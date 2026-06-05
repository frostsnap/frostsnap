//! BIP-322 "simple" message signing for P2TR key-path addresses.
//!
//! The sighash is signed with the taproot output key that
//! [`AppTweak::Bitcoin`](crate::tweak::AppTweak) derives, so a BIP-322 sign item is
//! the same as signing one taproot input.
//!
//! <https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki>

use bitcoin::{
    absolute::LockTime,
    blockdata::transaction::Version,
    hashes::{sha256, Hash, HashEngine},
    opcodes,
    script::{self, PushBytes},
    sighash::{Prevouts, SighashCache, TapSighashType},
    Amount, OutPoint, ScriptBuf, Sequence, TapSighash, Transaction, TxIn, TxOut, Txid, Witness,
};

/// Sparrow's verifier always recomputes the sighash with `SIGHASH_ALL`, and the
/// spec's taproot test vector uses it too, so the witness signature is 65 bytes
/// with a trailing `0x01`.
const SIGHASH_TYPE: TapSighashType = TapSighashType::All;

/// The `to_sign` sighash that the key behind `spk` signs for `message`.
pub fn sighash(spk: ScriptBuf, message: &str) -> TapSighash {
    let tag = sha256::Hash::hash(b"BIP0322-signed-message");
    let mut engine = sha256::Hash::engine();
    engine.input(tag.as_ref());
    engine.input(tag.as_ref());
    engine.input(message.as_bytes());
    let message_hash = sha256::Hash::from_engine(engine).to_byte_array();
    let message_hash: &PushBytes = message_hash.as_slice().try_into().expect("32 bytes");

    let to_spend = Transaction {
        version: Version(0),
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: Txid::all_zeros(),
                vout: 0xFFFF_FFFF,
            },
            script_sig: script::Builder::new()
                .push_int(0)
                .push_slice(message_hash)
                .into_script(),
            sequence: Sequence(0),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::ZERO,
            script_pubkey: spk,
        }],
    };
    let to_sign = Transaction {
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
    };
    SighashCache::new(&to_sign)
        .taproot_key_spend_signature_hash(
            0,
            &Prevouts::All(to_spend.output.as_slice()),
            SIGHASH_TYPE,
        )
        .expect("to_sign has exactly one input")
}

/// The witness of a BIP-322 simple signature.
pub fn witness(signature: &[u8; 64]) -> Witness {
    let mut element = signature.to_vec();
    element.push(SIGHASH_TYPE as u8);
    Witness::from_slice(&[element])
}
