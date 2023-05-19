use schnorr_fun::fun::{hex, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct ExtendedPubKey(pub [u8; 65]);

impl ExtendedPubKey {
    pub fn new(public_key: Point, chain_code: [u8; 32]) -> Self {
        let mut key = [0u8; 65];
        key[0..33].copy_from_slice(public_key.to_bytes().as_ref());
        key[33..65].copy_from_slice(&chain_code);
        Self(key)
    }
}

impl core::fmt::Display for ExtendedPubKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}
