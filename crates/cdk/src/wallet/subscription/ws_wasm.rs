use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use cdk_common::subscription::Params;
use cdk_common::ws::{WsMessageOrResponse, WsMethodRequest, WsRequest, WsUnsubscribeRequest};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, RwLock};
// Remove unused imports
use ws_stream_wasm::{WsMessage, WsMeta};

use super::http::http_main;
use super::WsSubscriptionBody;
use crate::mint_url::MintUrl;
use crate::pub_sub::SubId;
use crate::wallet::MintConnector;
use crate::Wallet;

const MAX_ATTEMPT_FALLBACK_HTTP: usize = 10;

async fn fallback_to_http<S: IntoIterator<Item = SubId>>(
    initial_state: S,
    http_client: Arc<dyn MintConnector + Send + Sync>,
    subscriptions: Arc<RwLock<HashMap<SubId, WsSubscriptionBody>>>,
    new_subscription_recv: mpsc::Receiver<SubId>,
    on_drop: mpsc::Receiver<SubId>,
    wallet: Arc<Wallet>,
) {
    http_main(
        initial_state,
        http_client,
        subscriptions,
        new_subscription_recv,
        on_drop,
        wallet,
    )
    .await
}

#[inline]
pub async fn ws_main(
    http_client: Arc<dyn MintConnector + Send + Sync>,
    mint_url: MintUrl,
    subscriptions: Arc<RwLock<HashMap<SubId, WsSubscriptionBody>>>,
    mut new_subscription_recv: mpsc::Receiver<SubId>,
    mut on_drop: mpsc::Receiver<SubId>,
    wallet: Arc<Wallet>,
) {
    let mut url = mint_url
        .join_paths(&["v1", "ws"])
        .expect("Could not join paths");

    if url.scheme() == "https" {
        url.set_scheme("wss").expect("Could not set scheme");
    } else {
        url.set_scheme("ws").expect("Could not set scheme");
    }

    let url = url.to_string();

    let mut active_subscriptions = HashMap::<SubId, mpsc::Sender<_>>::new();
    let mut failure_count = 0;

    loop {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::console;
            console::log_1(&format!("🔌 WebSocket: Attempting to connect to {}", url).into());
        }
        tracing::debug!("Connecting to {}", url);

        // Create WebSocket connection using ws_stream_wasm
        let (_ws_meta, ws_stream) = match WsMeta::connect(&url, None).await {
            Ok(connection) => {
                #[cfg(target_arch = "wasm32")]
                {
                    use web_sys::console;
                    console::log_1(
                        &format!("✅ WebSocket: Successfully connected to {}", url).into(),
                    );
                }
                connection
            }
            Err(err) => {
                failure_count += 1;
                #[cfg(target_arch = "wasm32")]
                {
                    use web_sys::console;
                    console::error_1(
                        &format!(
                            "❌ WebSocket: Connection failed (attempt {}): {:?}",
                            failure_count, err
                        )
                        .into(),
                    );
                }
                tracing::error!("Could not connect to server: {:?}", err);
                if failure_count > MAX_ATTEMPT_FALLBACK_HTTP {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use web_sys::console;
                        console::error_1(&format!("🔄 WebSocket: Too many failures ({}), falling back to HTTP polling", MAX_ATTEMPT_FALLBACK_HTTP).into());
                    }
                    tracing::error!(
                        "Could not connect to server after {MAX_ATTEMPT_FALLBACK_HTTP} attempts, falling back to HTTP-subscription client"
                    );
                    return fallback_to_http(
                        active_subscriptions.into_keys(),
                        http_client,
                        subscriptions,
                        new_subscription_recv,
                        on_drop,
                        wallet,
                    )
                    .await;
                }
                continue;
            }
        };

        tracing::debug!("Connected to {}", url);
        failure_count = 0;
        tracing::debug!("Reset failure count to {}", failure_count);

        let (mut write, mut read) = ws_stream.split();
        let req_id = AtomicUsize::new(0);

        let get_sub_request = |params: Params| -> Option<(usize, String)> {
            let request: WsRequest = (
                WsMethodRequest::Subscribe(params),
                req_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            )
                .into();

            match serde_json::to_string(&request) {
                Ok(json) => Some((request.id, json)),
                Err(err) => {
                    tracing::error!("Could not serialize subscribe message: {:?}", err);
                    None
                }
            }
        };

        let get_unsub_request = |sub_id: SubId| -> Option<String> {
            let request: WsRequest = (
                WsMethodRequest::Unsubscribe(WsUnsubscribeRequest { sub_id }),
                req_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            )
                .into();

            match serde_json::to_string(&request) {
                Ok(json) => Some(json),
                Err(err) => {
                    tracing::error!("Could not serialize unsubscribe message: {:?}", err);
                    None
                }
            }
        };

        // WebSocket reconnected, restore all subscriptions
        let mut subscription_requests = HashSet::new();

        let read_subscriptions = subscriptions.read().await;
        for (sub_id, _) in active_subscriptions.iter() {
            if let Some(Some((req_id, req))) = read_subscriptions
                .get(sub_id)
                .map(|(_, params)| get_sub_request(params.clone()))
            {
                let _ = write.send(WsMessage::Text(req)).await;
                subscription_requests.insert(req_id);
            }
        }
        drop(read_subscriptions);

        loop {
            tokio::select! {
                Some(msg) = read.next() => {
                    let text = match msg {
                        WsMessage::Text(text) => text,
                        WsMessage::Binary(_) => continue, // Skip binary messages
                    };

                    let msg = match serde_json::from_str::<WsMessageOrResponse>(&text) {
                        Ok(msg) => msg,
                        Err(_) => continue,
                    };

                    match msg {
                        WsMessageOrResponse::Notification(payload) => {
                            tracing::debug!("Received notification from server: {:?}", payload);
                            let _ = active_subscriptions.get(&payload.params.sub_id).map(|sender| {
                                let _ = sender.try_send(payload.params.payload);
                            });
                        }
                        WsMessageOrResponse::Response(response) => {
                            tracing::debug!("Received response from server: {:?}", response);
                            subscription_requests.remove(&response.id);
                        }
                        WsMessageOrResponse::ErrorResponse(error) => {
                            tracing::error!("Received error from server: {:?}", error);
                            if subscription_requests.contains(&error.id) {
                                // If the server sends an error response to a subscription request, we should
                                // fallback to HTTP.
                                // TODO: Add some retry before giving up to HTTP.
                                return fallback_to_http(
                                    active_subscriptions.into_keys(),
                                    http_client,
                                    subscriptions,
                                    new_subscription_recv,
                                    on_drop,
                                    wallet
                                ).await;
                            }
                        }
                    }

                }
                Some(subid) = new_subscription_recv.recv() => {
                    let subscription = subscriptions.read().await;
                    let sub = if let Some(subscription) = subscription.get(&subid) {
                        subscription
                    } else {
                        continue
                    };
                    tracing::debug!("Subscribing to {:?}", sub.1);
                    active_subscriptions.insert(subid, sub.0.clone());
                    if let Some((req_id, json)) = get_sub_request(sub.1.clone()) {
                        let _ = write.send(WsMessage::Text(json)).await;
                        subscription_requests.insert(req_id);
                    }
                },
                Some(subid) = on_drop.recv() => {
                    let mut subscription = subscriptions.write().await;
                    if let Some(sub) = subscription.remove(&subid) {
                        drop(sub);
                    }
                    tracing::debug!("Unsubscribing from {:?}", subid);
                    let subid_clone = subid.clone();
                    if let Some(json) = get_unsub_request(subid) {
                        let _ = write.send(WsMessage::Text(json)).await;
                    }
                    active_subscriptions.remove(&subid_clone);
                }
            }
        }
    }
}
