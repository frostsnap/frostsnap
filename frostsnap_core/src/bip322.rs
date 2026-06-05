//! BIP-322 "simple" message signing primitives for P2TR key-path addresses.
//!
//! These build the BIP-322 `to_spend`/`to_sign` virtual transactions and compute
//! the taproot key-spend sighash that must be signed. The construction is
//! deterministic and `no_std` so the device and coordinator compute identical
//! sighashes from a [`WireSignTask::Bip322`](crate::WireSignTask).
//!
//! For a P2TR key-path address the resulting sighash is signed with the taproot
//! output key — exactly the key [`AppTweak::Bitcoin`](crate::tweak::AppTweak) derives —
//! so a BIP-322 sign item is structurally identical to signing one taproot input.
//!
//! Reference: <https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki>

use alloc::vec;
use bitcoin::{
    absolute::LockTime,
    blockdata::transaction::Version,
    hashes::{sha256, Hash, HashEngine},
    opcodes,
    script::{self, PushBytes},
    sighash::{Prevouts, SighashCache, TapSighashType},
    Amount, OutPoint, ScriptBuf, Sequence, TapSighash, Transaction, TxIn, TxOut, Txid, Witness,
};

/// The BIP-322 message tag.
pub const BIP322_TAG: &str = "BIP0322-signed-message";

/// `to_sign` commits with `SIGHASH_ALL`. This is the canonical BIP-322 taproot
/// form (the spec's taproot test vector is a 65-byte `SIGHASH_ALL` signature)
/// and the one ecosystem verifiers expect — notably Sparrow, whose verifier
/// always recomputes the sighash with `SIGHASH_ALL`. The witness signature is
/// therefore 64 signature bytes followed by the `0x01` sighash-type byte.
pub const SIGHASH_TYPE: TapSighashType = TapSighashType::All;

/// The single witness element of a BIP-322 simple signature: the 64-byte
/// Schnorr signature followed by the [`SIGHASH_TYPE`] byte.
pub fn witness_element(signature: &[u8; 64]) -> alloc::vec::Vec<u8> {
    let mut element = signature.to_vec();
    element.push(SIGHASH_TYPE as u8);
    element
}

/// Compute the BIP-322 tagged message hash: `SHA256(SHA256(tag) || SHA256(tag) || message)`.
pub fn message_hash(message: &str) -> [u8; 32] {
    let tag_hash = sha256::Hash::hash(BIP322_TAG.as_bytes());
    let mut engine = sha256::Hash::engine();
    engine.input(tag_hash.as_ref());
    engine.input(tag_hash.as_ref());
    engine.input(message.as_bytes());
    sha256::Hash::from_engine(engine).to_byte_array()
}

/// Build the BIP-322 `to_spend` virtual transaction for an address `spk`.
fn build_to_spend(spk: &ScriptBuf, message: &str) -> Transaction {
    let msg_hash = message_hash(message);
    let push: &PushBytes = msg_hash.as_slice().try_into().expect("hash is 32 bytes");
    Transaction {
        version: Version(0),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::all_zeros(),
                vout: 0xFFFF_FFFF,
            },
            script_sig: script::Builder::new()
                .push_int(0)
                .push_slice(push)
                .into_script(),
            sequence: Sequence(0),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::ZERO,
            script_pubkey: spk.clone(),
        }],
    }
}

/// Build the BIP-322 `to_sign` virtual transaction spending `to_spend`'s output.
fn build_to_sign(to_spend: &Transaction) -> Transaction {
    Transaction {
        version: Version(0),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: to_spend.compute_txid(),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence(0),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::ZERO,
            script_pubkey: script::Builder::new()
                .push_opcode(opcodes::all::OP_RETURN)
                .into_script(),
        }],
    }
}

/// Compute the `to_sign` input-0 taproot key-spend sighash for the given address
/// `spk` and `message`. This is the message the FROST key behind the address signs.
pub fn to_sign_sighash(spk: &ScriptBuf, message: &str) -> TapSighash {
    let to_spend = build_to_spend(spk, message);
    let to_sign = build_to_sign(&to_spend);
    let prevout = to_spend.output[0].clone();
    SighashCache::new(&to_sign)
        .taproot_key_spend_signature_hash(0, &Prevouts::All(&[prevout]), SIGHASH_TYPE)
        .expect("BIP-322 to_sign has exactly one input")
}

/// The sighash bytes for use as a [`SignItem`](crate::SignItem) message.
pub fn sighash_bytes(spk: &ScriptBuf, message: &str) -> [u8; 32] {
    to_sign_sighash(spk, message).to_byte_array()
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::hex::DisplayHex;

    // Test vectors from the bip322 crate / BIP-322 spec.
    #[test]
    fn message_hashes_match_spec() {
        assert_eq!(
            message_hash("").to_lower_hex_string(),
            "c90c269c4f8fcbe6880f72a721ddfbf1914268a794cbb21cfafee13770ae19f1"
        );
        assert_eq!(
            message_hash("Hello World").to_lower_hex_string(),
            "f0eb03b1a75ac6d9847f55c624a99169b5dccba2a31f5b23bea77ba270de0a7a"
        );
    }

    /// Cross-check: a taproot key-spend signature over our independently-built
    /// sighash must verify under the independent `bip322` crate. This guards the
    /// exact virtual-tx field values (version/locktime/sequence/OP_RETURN) and the
    /// bare 64-byte (SIGHASH_DEFAULT) witness encoding.
    #[test]
    fn taproot_simple_signature_verifies_against_bip322_crate() {
        use bitcoin::key::{Keypair, TapTweak};
        use bitcoin::secp256k1::{Message, Secp256k1, SecretKey};
        use bitcoin::{Address, Network, Witness};

        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(&[0x42u8; 32]).unwrap();
        let keypair = Keypair::from_secret_key(&secp, &sk);
        let (internal_key, _parity) = keypair.x_only_public_key();
        let address = Address::p2tr(&secp, internal_key, None, Network::Bitcoin);
        let spk = address.script_pubkey();

        for message in ["", "Hello World", "frostsnap signs bip322"] {
            let sighash = sighash_bytes(&spk, message);
            let tweaked = keypair.tap_tweak(&secp, None).to_keypair();
            let sig = secp.sign_schnorr_no_aux_rand(&Message::from_digest(sighash), &tweaked);
            let witness = Witness::from_slice(&[witness_element(&sig.serialize())]);
            bip322::verify_simple(&address, message, witness)
                .unwrap_or_else(|e| panic!("bip322 verify failed for {message:?}: {e}"));
        }
    }
}
