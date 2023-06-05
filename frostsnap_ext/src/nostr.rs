use alloc::string::{String, ToString};
use alloc::vec::Vec;

use schnorr_fun::fun::{marker::EvenY, Point};
use schnorr_fun::Signature;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnsignedEvent {
    pub id: String,
    pubkey: Point<EvenY>,
    created_at: i64,
    kind: u64,
    tags: Vec<Vec<String>>,
    pub content: String,
    pub hash_bytes: Vec<u8>,
}

impl UnsignedEvent {
    pub fn new(
        pubkey: Point<EvenY>,
        kind: u64,
        tags: Vec<Vec<String>>,
        content: String,
        created_at: i64,
    ) -> Self {
        let serialized_event = json!([0, pubkey, created_at, kind, json!(tags), content]);

        let mut hash = Sha256::default();
        hash.update(serialized_event.to_string().as_bytes());
        let hash_result = hash.finalize();
        let hash_result_str = format!("{:x}", hash_result);

        Self {
            id: hash_result_str,
            pubkey,
            created_at,
            kind,
            tags,
            content,
            hash_bytes: hash_result.to_vec(),
        }
    }

    pub fn add_signature(self, signature: Signature) -> Event {
        Event {
            id: self.id,
            pubkey: self.pubkey,
            created_at: self.created_at,
            kind: self.kind,
            tags: self.tags,
            content: self.content,
            sig: signature,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub id: String,
    pubkey: Point<EvenY>,
    created_at: i64,
    kind: u64,
    tags: Vec<Vec<String>>,
    pub content: String,
    sig: Signature,
}

#[cfg(all(feature = "tungstenite", feature = "anyhow"))]
pub fn broadcast_event(event: Event, relay: &str) -> anyhow::Result<()> {
    let (mut socket, _) = tungstenite::connect(relay)?;
    let msg = json!(["EVENT", event]).to_string();
    socket.write_message(tungstenite::Message::Text(msg))?;
    Ok(())
}
