use alloc::string::String;
use alloc::vec::Vec;

use schnorr_fun::fun::hex;
use schnorr_fun::fun::{marker::EvenY, Point};
use schnorr_fun::Signature;

pub fn get_npub(public_key: Point<EvenY>) -> String {
    bitcoin::bech32::encode(
        "npub",
        bitcoin::bech32::ToBase32::to_base32(&public_key.to_xonly_bytes()),
        bitcoin::bech32::Variant::Bech32,
    )
    .unwrap()
}

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
        // let hash_result: [u8; 32] = hash.finalize().into();
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

    pub fn add_signature(&self, signature: Signature) -> Event {
        Event {
            id: self.id.clone(),
            pubkey: hex::encode(&self.pubkey.to_xonly_bytes()),
            created_at: self.created_at,
            kind: self.kind,
            tags: self.tags.clone(),
            content: self.content.clone(),
            sig: hex::encode(&signature.to_bytes()),
        }
    }

    pub fn note_id(&self) -> String {
        let id_bytes = hex::decode(&self.id).expect("just created valid bytes");
        bitcoin::bech32::encode(
            "note",
            bitcoin::bech32::ToBase32::to_base32(&id_bytes),
            bitcoin::bech32::Variant::Bech32,
        )
        .unwrap()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub id: String,
    pubkey: String,
    created_at: i64,
    kind: u64,
    tags: Vec<Vec<String>>,
    pub content: String,
    sig: String,
}

#[cfg(feature = "serde_json")]
impl Event {
    pub fn to_json_string(&self) -> String {
        use alloc::string::ToString;
        serde_json::json!(&self).to_string()
    }

    pub fn to_websocket_msg(&self) -> String {
        use alloc::string::ToString;
        serde_json::json!(["EVENT", self]).to_string()
    }
}
