use crate::nostr::UnsignedEvent;
use alloc::{string::String, vec::Vec};
use bdk_chain::bitcoin::{
    self, util::sighash::SighashCache, SchnorrSighashType, Transaction, TxOut,
};

// The coordinator will have some <transaction> with N inputs, requiring N signatures.
/// We want to pass the entire transaction (PSBT or otherwise) through to the device,
/// Such that the device can validate the [address, amount] pairs.
/// (Can validate more data, e.g. txid, but this will do for now).
///
/// So we will need to make a signature request with some [`SignRequestMessage`],
/// that is an instruction to sign N signatures.

/// Currently a RequestSign looks like
///
/// RequestSign {
///     nonces: BTreeMap<DeviceId, (Vec<Nonce>, usize, usize)>,
///     messages_to_sign: Vec<Vec<u8>>,
///     tap_tweak: bool,
/// },
///
/// Do we still want the functionality of signing multiple messages at once?
/// E.g. Signing two independent transactions Vec<PSBT> at once?
/// Or just a single PSBT which contains multiple messages?
///
/// I think we still want to sign a Vec of messages. I.e. a Vec of PSBTs of inputs:
/// Vec<RequestSignMessage>
///

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RequestSignMessage {
    Plain(Vec<u8>),       // 1 nonce & sig
    Nostr(UnsignedEvent), // 1 nonce & sig
    Transaction {
        tx_template: Transaction,
        prevouts: Vec<TxOut>,
    }, // N nonces and sigs
}

/// For M [`RequestSignMessage::Psbt`]s we would need M*N nonces to produce M*N signatures
/// Should we allow a device to reject some proportion of the M signing requests?

// What to show on the device for signing requests
impl core::fmt::Display for RequestSignMessage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RequestSignMessage::Plain(message) => {
                write!(f, "Plain:{}", String::from_utf8_lossy(message))
            }
            RequestSignMessage::Nostr(event) => write!(f, "Nostr: {}", event.content),
            RequestSignMessage::Transaction { .. } => write!(f, "Tx:{:?}", "dog"),
        }
    }
}

impl RequestSignMessage {
    pub fn message_chunks_to_sign(self) -> Vec<Vec<u8>> {
        match self {
            RequestSignMessage::Plain(message) => vec![message],
            RequestSignMessage::Nostr(event) => vec![event.hash_bytes],
            RequestSignMessage::Transaction {
                tx_template,
                prevouts,
            } => {
                let mut tx_sighashes = vec![];
                let _sighash_tx = tx_template.clone();
                let schnorr_sighashty = SchnorrSighashType::Default;
                for (i, _) in tx_template.input.iter().enumerate() {
                    let mut sighash_cache = SighashCache::new(&_sighash_tx);
                    let sighash = sighash_cache
                        .taproot_key_spend_signature_hash(
                            i,
                            &bitcoin::psbt::Prevouts::All(&prevouts),
                            schnorr_sighashty,
                        )
                        .unwrap(); // TODO remove unwrap
                    tx_sighashes.push(sighash);
                }
                let messages = tx_sighashes
                    .into_iter()
                    .map(|sighash| sighash.to_vec())
                    .collect();

                messages
            }
        }
    }
}
