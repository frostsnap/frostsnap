pub use nostr::{prelude::*, secp256k1::schnorr::Signature};
use tungstenite::Message as WsMessage;

pub fn create_unsigned_nostr_event(pk: String, message: &str) -> anyhow::Result<UnsignedEvent> {
    let my_keys = Keys::from_pk_str(&pk)?;
    let event = EventBuilder::new_text_note(message, &[]).to_unsigned_event(my_keys.public_key());
    Ok(event)
}

pub fn add_signature(
    event: UnsignedEvent,
    signature: frostsnap_core::schnorr_fun::Signature,
) -> anyhow::Result<Event> {
    let bytes = signature.to_bytes();
    let nostr_signature = Signature::from_slice(&bytes)?;
    let signed_event = event.add_signature(nostr_signature)?;
    Ok(signed_event)
}

pub fn broadcast_event(event: Event, relay: &str) -> anyhow::Result<()> {
    let (mut socket, _) = tungstenite::connect(relay)?;
    let msg = ClientMessage::new_event(event).as_json();
    socket.write_message(WsMessage::Text(msg))?;
    Ok(())
}
