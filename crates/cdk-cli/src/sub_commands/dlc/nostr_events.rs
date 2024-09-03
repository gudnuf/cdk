use std::vec;

use crate::sub_commands::dlc::UserBet;
use nostr_sdk::event::builder::Error;
use nostr_sdk::nips::nip04;
use nostr_sdk::{base64, Client, Event, EventBuilder, EventId, Filter, Keys, Kind, PublicKey, Tag};
/// Create Kind 8_888 event tagged with the counterparty pubkey
///
/// see https://github.com/nostr-protocol/nips/blob/9157321a224bca77b3472a19de72885af9d6a91d/88.md#kind8_888
///
/// # Arguments
/// * `keys` - The Keys used to sign the event
/// * `msg` - The dlc message
/// * `counterparty_pubkey` - Public key to send this message to
pub fn create_dlc_msg_event(
    keys: &Keys,
    msg: String,
    counterparty_pubkey: &PublicKey,
) -> Result<Event, Error> {
    // The DLC message is first serialized in binary, and then encrypted with NIP04.
    let content = base64::encode(msg);

    let content: String =
        nip04::encrypt(&keys.secret_key()?.clone(), counterparty_pubkey, content)?;

    EventBuilder::new(
        Kind::Custom(8888),
        content,
        vec![Tag::public_key(*counterparty_pubkey)],
    )
    .to_event(keys)
}

pub async fn lookup_announcement_event(
    event_id: EventId,
    client: &Client,
) -> Option<Result<Event, Error>> {
    let filter = Filter::new().id(event_id).kind(Kind::Custom(88));
    let events = client
        .get_events_of(vec![filter], None)
        .await
        .expect("get_events_of failed");
    if events.is_empty() {
        return None;
    }
    Some(Ok(events.first().unwrap().clone()))
}

pub async fn list_dlc_offers(keys: &Keys, client: &Client) -> Option<Vec<UserBet>> {
    let filter = Filter::new()
        .kind(Kind::Custom(8888))
        .pubkey(keys.public_key());
    let events = client
        .get_events_of(vec![filter], None)
        .await
        .expect("get_events_of failed");

    if events.is_empty() {
        return None;
    }

    let offers = events
        .iter()
        .map(|e| {
            let decrypted = nostr_sdk::nips::nip04::decrypt(
                keys.secret_key().unwrap(),
                &e.pubkey,
                e.content.clone(),
            )
            .unwrap();

            let decoded = base64::decode(&decrypted).unwrap();
            let decoded_str = std::str::from_utf8(&decoded).unwrap();
            serde_json::from_str::<UserBet>(decoded_str).unwrap()
        })
        .collect();
    Some(offers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{Client, EventId, Keys};

    #[tokio::test]
    async fn test_lookup_announcement_event() {
        let announemtent_id =
            EventId::from_hex("d30e6c857a900ebefbf7dc3b678ead9215f4345476067e146ded973971286529")
                .unwrap();

        let client = Client::new(&Keys::generate());
        let relay = "wss://relay.damus.io";
        client.add_relay(relay.to_string()).await.unwrap();
        client.connect().await;
        let event = lookup_announcement_event(announemtent_id, &client)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.id, announemtent_id);
    }
}
