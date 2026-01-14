use frostsnap_core::KeyId;
use sha2::{Digest, Sha256};

/// Hash with a tagged prefix (matches frostsnap_core's internal prefix_hash pattern)
fn prefix_hash(prefix: &'static str, data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::default();
    hash.update((prefix.len() as u8).to_be_bytes());
    hash.update(prefix);
    hash.update(data);
    hash.finalize().into()
}

/// Keys derived from a KeyId for channel encryption and identification.
#[derive(Clone)]
pub struct ChannelKeys {
    /// Channel ID used for the `h` tag - relays index this for subscriptions
    pub channel_id: [u8; 32],
    /// Shared secret for NIP44-style encryption
    pub shared_secret: [u8; 32],
}

impl ChannelKeys {
    /// Derive channel keys from a KeyId.
    ///
    /// Both the channel_id and shared_secret are deterministically derived from the key_id,
    /// so all participants with the same key_id will derive the same values.
    pub fn from_key_id(key_id: &KeyId) -> Self {
        let channel_id: [u8; 32] = prefix_hash("NOSTR_CHANNEL_ID", &key_id.0);
        let shared_secret: [u8; 32] = prefix_hash("NOSTR_CHANNEL_SHARED_SECRET", &key_id.0);

        Self {
            channel_id,
            shared_secret,
        }
    }

    /// Get the channel ID as a hex string for use in tags
    pub fn channel_id_hex(&self) -> String {
        hex::encode(self.channel_id)
    }
}

// TODO: Implement encrypt_event and decrypt_event using NIP44 primitives
// with the shared_secret instead of ECDH

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_keys_deterministic() {
        let key_id = KeyId([0x42; 32]);
        let keys1 = ChannelKeys::from_key_id(&key_id);
        let keys2 = ChannelKeys::from_key_id(&key_id);

        assert_eq!(keys1.channel_id, keys2.channel_id);
        assert_eq!(keys1.shared_secret, keys2.shared_secret);
    }

    #[test]
    fn different_key_ids_produce_different_keys() {
        let key_id1 = KeyId([0x42; 32]);
        let key_id2 = KeyId([0x43; 32]);

        let keys1 = ChannelKeys::from_key_id(&key_id1);
        let keys2 = ChannelKeys::from_key_id(&key_id2);

        assert_ne!(keys1.channel_id, keys2.channel_id);
        assert_ne!(keys1.shared_secret, keys2.shared_secret);
    }

    #[test]
    fn channel_id_and_shared_secret_are_different() {
        let key_id = KeyId([0x42; 32]);
        let keys = ChannelKeys::from_key_id(&key_id);

        assert_ne!(keys.channel_id, keys.shared_secret);
    }
}
