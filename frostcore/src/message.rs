//! A collection of communication enums for FROST gey generation and signing
//! Includes ability to write acks out with [`FrostMessage`]s

use schnorr_fun::{
    fun::{
        digest::generic_array::typenum::U32,
        marker::{EvenY, Public, Secret, Zero},
        Point, Scalar, XOnlyKeyPair,
    },
    nonce::NonceGen,
    Message, Schnorr, Signature,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct FrostSetup {
    pub n_parties: usize,
    pub threshold: usize,
    pub our_index: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Ack {
    Ack,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SetupMessage {
    ShareConfig(FrostSetup),
    NackConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KeygenPolyMessage {
    Polynomial(Vec<Point>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KeygenSharesMessage {
    SecretShares(Vec<Scalar<Secret, Zero>>, Signature),
    UnacceptedPoly(usize),
    InvalidProofOfPossession(usize),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SigningMessage {
    ShareNonce,
    ShareSignatureShare,
    UnacceptedNonce(usize),
    InvalidSignatureShare(usize),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageItem {
    SetupMessage(SetupMessage),
    KeygenPolyMessage(KeygenPolyMessage),
    KeygenSharesMessage(KeygenSharesMessage),
    SigningMessage(SigningMessage),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrostMessage {
    pub message: MessageItem,
    pub signature: Signature,
    pub sender: Point<EvenY>,
    pub continue_ack: bool,
}

impl FrostMessage {
    pub fn new<NG: NonceGen, CH: Digest<OutputSize = U32> + Clone>(
        schnorr: &Schnorr<CH, NG>,
        keypair: &XOnlyKeyPair,
        message: MessageItem,
    ) -> Self {
        Self {
            message: message.clone(),
            signature: schnorr.sign(
                &keypair,
                Message::<Public>::raw(&bincode::serialize(&message).unwrap()),
            ),
            sender: keypair.public_key(),
            continue_ack: false,
        }
    }

    // We are going to continually write messages to serial until everyone receives enough
    // messages "continue_acks". This may not be appropriate in more reliable public boards.
    pub fn ready_to_continue(self) -> FrostMessage {
        FrostMessage {
            message: self.message,
            signature: self.signature,
            sender: self.sender,
            continue_ack: true,
        }
    }
}
