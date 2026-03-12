use frostsnap_core::schnorr_fun::frost::SharedKey;
use frostsnap_core::{
    coordinator::KeyContext, device::KeyPurpose, AccessStructureId, AccessStructureRef, KeyId,
    MasterAppkey,
};
use sha2::{Digest, Sha256};

fn prefix_hash(prefix: &'static str, data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::default();
    hash.update((prefix.len() as u8).to_be_bytes());
    hash.update(prefix);
    hash.update(data);
    hash.finalize().into()
}

/// A compact 16-byte secret derived from an AccessStructureId.
/// This is what gets shared in invite links. From it you can derive
/// the channel_id and shared_secret needed to find and decrypt channel messages.
#[derive(Clone, Debug)]
pub struct ChannelSecret(pub [u8; 16]);

impl ChannelSecret {
    pub fn from_access_structure_id(id: &AccessStructureId) -> Self {
        // 🧪 bump counter to create fresh channels during testing
        let hash = prefix_hash("NOSTR_CHANNEL_SECRET/2", &id.0);
        let mut secret = [0u8; 16];
        secret.copy_from_slice(&hash[..16]);
        ChannelSecret(secret)
    }

    pub fn invite_link(&self) -> String {
        format!("frostsnap://channel/{}", hex::encode(self.0))
    }
}

/// Keys derived from a ChannelSecret for channel encryption and identification.
#[derive(Clone)]
pub struct ChannelKeys {
    /// Channel ID used for the `h` tag - relays index this for subscriptions
    pub channel_id: [u8; 32],
    /// Shared secret for NIP44-style encryption
    pub shared_secret: [u8; 32],
}

impl ChannelKeys {
    pub fn from_channel_secret(secret: &ChannelSecret) -> Self {
        let channel_id = prefix_hash("NOSTR_CHANNEL_ID", &secret.0);
        let shared_secret = prefix_hash("NOSTR_CHANNEL_SHARED_SECRET", &secret.0);
        Self {
            channel_id,
            shared_secret,
        }
    }

    pub fn from_access_structure_id(id: &AccessStructureId) -> Self {
        Self::from_channel_secret(&ChannelSecret::from_access_structure_id(id))
    }

    /// Get the channel ID as a hex string for use in tags
    pub fn channel_id_hex(&self) -> String {
        hex::encode(self.channel_id)
    }
}

/// Key metadata published in the NIP28 channel creation event.
/// Contains just enough to reconstruct the key in a coordinator.
#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct ChannelInitData {
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub root_shared_key: SharedKey,
}

impl ChannelInitData {
    pub fn key_id(&self) -> KeyId {
        KeyId::from_rootkey(self.root_shared_key.public_key())
    }

    pub fn master_appkey(&self) -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(self.root_shared_key.public_key())
    }

    pub fn access_structure_id(&self) -> AccessStructureId {
        AccessStructureId::from_root_shared_key(&self.root_shared_key)
    }

    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef::from_root_shared_key(&self.root_shared_key)
    }

    pub fn key_context(&self) -> KeyContext {
        let app_shared_key =
            frostsnap_core::tweak::Xpub::from_rootkey(self.root_shared_key.clone())
                .rootkey_to_master_appkey();
        KeyContext {
            app_shared_key,
            purpose: self.purpose,
        }
    }
}

/// Parse a `frostsnap://channel/<channel_secret_hex>` link into a ChannelSecret.
pub fn parse_frostsnap_link(url: &str) -> Result<ChannelSecret, String> {
    let hex_str = url.strip_prefix("frostsnap://channel/").ok_or_else(|| {
        "invalid frostsnap link: expected frostsnap://channel/<secret_hex>".to_string()
    })?;
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex in frostsnap link: {e}"))?;
    if bytes.len() != 16 {
        return Err(format!(
            "invalid channel secret length: expected 16 bytes, got {}",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes);
    Ok(ChannelSecret(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_keys_deterministic() {
        let as_id = AccessStructureId([0x42; 32]);
        let keys1 = ChannelKeys::from_access_structure_id(&as_id);
        let keys2 = ChannelKeys::from_access_structure_id(&as_id);

        assert_eq!(keys1.channel_id, keys2.channel_id);
        assert_eq!(keys1.shared_secret, keys2.shared_secret);
    }

    #[test]
    fn different_ids_produce_different_keys() {
        let as_id1 = AccessStructureId([0x42; 32]);
        let as_id2 = AccessStructureId([0x43; 32]);

        let keys1 = ChannelKeys::from_access_structure_id(&as_id1);
        let keys2 = ChannelKeys::from_access_structure_id(&as_id2);

        assert_ne!(keys1.channel_id, keys2.channel_id);
        assert_ne!(keys1.shared_secret, keys2.shared_secret);
    }

    #[test]
    fn channel_id_and_shared_secret_are_different() {
        let as_id = AccessStructureId([0x42; 32]);
        let keys = ChannelKeys::from_access_structure_id(&as_id);

        assert_ne!(keys.channel_id, keys.shared_secret);
    }

    #[test]
    fn channel_secret_roundtrip_via_link() {
        let as_id = AccessStructureId([0x42; 32]);
        let secret = ChannelSecret::from_access_structure_id(&as_id);
        let link = secret.invite_link();
        let parsed = parse_frostsnap_link(&link).unwrap();
        assert_eq!(secret.0, parsed.0);
    }
}
