use frostsnap_ext::nostr::Event;

pub fn broadcast_event(event: Event, relay: &str) -> anyhow::Result<()> {
    let (mut socket, _) = tungstenite::connect(relay)?;
    let msg = serde_json::json!(["EVENT", event]).to_string();
    socket.write_message(tungstenite::Message::Text(msg))?;
    Ok(())
}
