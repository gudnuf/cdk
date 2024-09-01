use std::vec;

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
