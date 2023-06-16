use alloc::string::String;
use alloc::vec::Vec;

use schnorr_fun::fun::{marker::EvenY, Point};
use schnorr_fun::Signature;

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd,
)]
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
    /// HACK: we only need `new` on the coordinator and serde_json doesn't work with no_std so we feature gate `new`.
    #[cfg(feature = "serde_json")]
    pub fn new(
        pubkey: Point<EvenY>,
        kind: u64,
        tags: Vec<Vec<String>>,
        content: String,
        created_at: i64,
    ) -> Self {
        use alloc::string::ToString;
        use sha2::Digest;
        use sha2::Sha256;

        let serialized_event = serde_json::json!([
            0,
            pubkey,
            created_at,
            kind,
            serde_json::json!(tags),
            content
        ]);

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
