use cdk_common::amount::SplitTarget;
use cdk_common::wallet::{MeltQuote, MintQuote};
use cdk_common::{
    Amount, Error, MeltQuoteState, MintQuoteState, NotificationPayload, PaymentMethod, Proofs,
    SpendingConditions,
};
#[cfg(not(target_arch = "wasm32"))]
use futures::future::BoxFuture;
#[cfg(target_arch = "wasm32")]
use futures::future::LocalBoxFuture as BoxFuture;
use instant::{Duration, Instant};

use super::{Wallet, WalletSubscription};

#[allow(private_bounds)]
#[allow(clippy::enum_variant_names)]
enum WaitableEvent {
    MeltQuote(String),
    MintQuote(String),
    Bolt12MintQuote(String),
}

impl From<&MeltQuote> for WaitableEvent {
    fn from(event: &MeltQuote) -> Self {
        WaitableEvent::MeltQuote(event.id.to_owned())
    }
}

impl From<&MintQuote> for WaitableEvent {
    fn from(event: &MintQuote) -> Self {
        match event.payment_method {
            PaymentMethod::Bolt11 => WaitableEvent::MintQuote(event.id.to_owned()),
            PaymentMethod::Bolt12 => WaitableEvent::Bolt12MintQuote(event.id.to_owned()),
            PaymentMethod::Custom(_) => WaitableEvent::MintQuote(event.id.to_owned()),
        }
    }
}

impl From<WaitableEvent> for WalletSubscription {
    fn from(val: WaitableEvent) -> Self {
        match val {
            WaitableEvent::MeltQuote(quote_id) => {
                WalletSubscription::Bolt11MeltQuoteState(vec![quote_id])
            }
            WaitableEvent::MintQuote(quote_id) => {
                WalletSubscription::Bolt11MintQuoteState(vec![quote_id])
            }
            WaitableEvent::Bolt12MintQuote(quote_id) => {
                WalletSubscription::Bolt12MintQuoteState(vec![quote_id])
            }
        }
    }
}

impl Wallet {
    #[inline(always)]
    /// Mints a mint quote once it is paid
    pub async fn wait_and_mint_quote(
        &self,
        quote: MintQuote,
        amount_split_target: SplitTarget,
        spending_conditions: Option<SpendingConditions>,
        timeout_duration: Duration,
    ) -> Result<Proofs, Error> {
        let amount = self.wait_for_payment(&quote, timeout_duration).await?;

        tracing::debug!("Received payment notification for {}. Minting...", quote.id);

        match quote.payment_method {
            PaymentMethod::Bolt11 => {
                self.mint(&quote.id, amount_split_target, spending_conditions)
                    .await
            }
            PaymentMethod::Bolt12 => {
                self.mint_bolt12(&quote.id, amount, amount_split_target, spending_conditions)
                    .await
            }
            _ => Err(Error::UnsupportedPaymentMethod),
        }
    }

    /// Returns a BoxFuture that will wait for payment on the given event with a timeout check
    /// This implementation is WASM-compatible using the instant crate
    #[allow(private_bounds)]
    pub fn wait_for_payment<T>(
        &self,
        event: T,
        timeout_duration: Duration,
    ) -> BoxFuture<'_, Result<Option<Amount>, Error>>
    where
        T: Into<WaitableEvent>,
    {
        let subs = self.subscribe::<WalletSubscription>(event.into().into());

        Box::pin(async move {
            let start_time = Instant::now();
            let mut subscription = subs.await;

            loop {
                // Check timeout using WASM-compatible instant
                if start_time.elapsed() > timeout_duration {
                    return Err(Error::Timeout);
                }

                // Try to receive with non-blocking approach
                match subscription.try_recv() {
                    Ok(Some(payload)) => match payload {
                        NotificationPayload::MintQuoteBolt11Response(info) => {
                            if info.state == MintQuoteState::Paid {
                                return Ok(None);
                            }
                        }
                        NotificationPayload::MintQuoteBolt12Response(info) => {
                            if info.amount_paid - info.amount_issued > Amount::ZERO {
                                return Ok(Some(info.amount_paid - info.amount_issued));
                            }
                        }
                        NotificationPayload::MeltQuoteBolt11Response(info) => {
                            if info.state == MeltQuoteState::Paid {
                                return Ok(None);
                            }
                        }
                        _ => {}
                    },
                    Ok(None) => {
                        // No message available, yield to allow other tasks to run
                        #[cfg(target_arch = "wasm32")]
                        wasm_bindgen_futures::spawn_local(async {});

                        #[cfg(not(target_arch = "wasm32"))]
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Err(_) => return Err(Error::Internal),
                }
            }
        })
    }
}
