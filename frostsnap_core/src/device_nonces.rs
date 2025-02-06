use alloc::collections::VecDeque;
use alloc::vec::Vec;
use chacha20::{
    cipher::{KeyIvInit, StreamCipher},
    ChaCha20,
};
use rand_core::RngCore;
use schnorr_fun::{
    binonce,
    frost::{NonceKeyPair, PairedSecretShare, PartySignSession, SignatureShare},
    fun::prelude::*,
};

use crate::{
    nonce_stream::{CoordNonceStreamState, NonceStreamId, NonceStreamSegment},
    SignSessionId, NONCE_BATCH_SIZE,
};

type ChaChaSeed = [u8; 32];

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SecretNonceSlot {
    //XXX: index must be first so we can decode it easily without decoding the whole thing
    //XXX: index == u32::MAX is invalid because we use it to represent "uninitialized"
    pub index: u32,
    pub nonce_stream_id: NonceStreamId,
    pub ratchet_prg_seed: ChaChaSeed,
    pub signing_state: Option<SigningState>,
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SigningState {
    pub session_id: SignSessionId,
    pub signature_shares: Vec<SignatureShare>,
}

pub trait NonceStreamSlot {
    fn read_index(&mut self) -> Option<u32>;
    fn read_slot(&mut self) -> Option<SecretNonceSlot>;
    fn write_slot(&mut self, value: &SecretNonceSlot);
}

/// Manages two writable sectors of persistent storage such that we make sure the state of the system we're managing is never lost.
/// The new state is first written, if that succeeds we finally write over the previous state.
/// In this case we're managing the secret seed needed to produce nonces for signatures.
#[derive(Clone, Debug, PartialEq)]
pub struct ABSlot<S> {
    slots: [S; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ABSlots<S> {
    slots: Vec<ABSlot<S>>,
}

impl<S: NonceStreamSlot> ABSlots<S> {
    pub fn new(ab_slots: impl IntoIterator<Item = ABSlot<S>>) -> Self {
        Self {
            slots: ab_slots.into_iter().collect(),
        }
    }

    pub fn get_or_create(
        &mut self,
        stream_id: NonceStreamId,
        rng: &mut impl RngCore,
    ) -> &mut ABSlot<S> {
        let mut i = 0;
        let found = loop {
            let ab_slot = &mut self.slots[i];
            match ab_slot.nonce_stream_id() {
                Some(slot_nsid) => {
                    if slot_nsid == stream_id {
                        break Some(i);
                    }
                }
                None => {
                    ab_slot.initialize(stream_id, rng);
                    break Some(i);
                }
            }
            i += 1;
            if i >= self.slots.len() {
                break None;
            }
        };

        match found {
            Some(i) => &mut self.slots[i],
            None => {
                let random_overwrite_idx = rng.next_u32() as usize % self.slots.len();
                let ab_slot = &mut self.slots[random_overwrite_idx];
                ab_slot.initialize(stream_id, rng);
                ab_slot
            }
        }
    }

    pub fn get(&mut self, stream_id: NonceStreamId) -> Option<&mut ABSlot<S>> {
        //XXX: clippy is wrong about this
        #[allow(clippy::manual_find)]
        for slot in &mut self.slots {
            if slot.nonce_stream_id() == Some(stream_id) {
                return Some(slot);
            }
        }
        None
    }

    pub fn all_stream_ids(&mut self) -> impl Iterator<Item = NonceStreamId> + '_ {
        self.slots
            .iter_mut()
            .filter_map(|slot| slot.nonce_stream_id())
    }
}

impl<S: NonceStreamSlot> ABSlot<S> {
    pub fn new(a: S, b: S) -> Self {
        Self { slots: [a, b] }
    }

    pub fn initialize(&mut self, stream_id: NonceStreamId, rng: &mut impl RngCore) {
        let mut ratchet_prg_seed = ChaChaSeed::default();
        rng.fill_bytes(&mut ratchet_prg_seed[..]);
        let value = SecretNonceSlot {
            index: 0,
            nonce_stream_id: stream_id,
            ratchet_prg_seed,
            signing_state: None,
        };
        for slot in &mut self.slots {
            slot.write_slot(&value);
        }
    }

    fn iter_pub_nonces(&mut self) -> impl Iterator<Item = (u32, binonce::Nonce)> {
        let current_slot = &mut self.slots[self.current_slot()];
        let slot_value = current_slot
            .read_slot()
            .expect("can't iter nonces of unintialized nonce slot");
        slot_value
            .iter_secret_nonces()
            .map(|(index, secret_nonce, _)| {
                (index, NonceKeyPair::from_secret(secret_nonce).public())
            })
    }

    pub fn reconcile_coord_nonce_stream_state(
        &mut self,
        state: CoordNonceStreamState,
    ) -> Option<NonceStreamSegment> {
        let our_index = self.current_index();
        if our_index > state.index || state.remaining < NONCE_BATCH_SIZE {
            Some(self.nonce_segment(None, NONCE_BATCH_SIZE as usize))
        } else if our_index == state.index {
            None
        } else {
            Some(self.nonce_segment(None, (state.index - our_index) as usize))
        }
    }

    pub fn nonce_segment(&mut self, start: Option<u32>, length: usize) -> NonceStreamSegment {
        let current_slot = &mut self.slots[self.current_slot()];
        let slot_value = current_slot
            .read_slot()
            .expect("can't iter nonces of unintialized nonce slot");
        let start = start.unwrap_or(slot_value.index);

        if start < slot_value.index {
            panic!("can't iterate erased nonces");
        }
        let last = start.saturating_add(length.try_into().expect("length not too big"));

        if last == u32::MAX {
            panic!("cannot have an index at u32::MAX");
        }

        let nonces = self
            .iter_pub_nonces()
            .skip((start - slot_value.index) as _)
            .map(|(_, nonce)| nonce)
            .take(length)
            .collect::<VecDeque<_>>();

        assert_eq!(nonces.len(), length, "there weren't enough noces for that");

        NonceStreamSegment {
            stream_id: slot_value.nonce_stream_id,
            index: start,
            nonces,
        }
    }

    pub fn are_nonces_available(&mut self, index: u32, n: u32) -> Result<(), NoncesUnavaillable> {
        let current_slot_value = self.slots[self.current_slot()]
            .read_slot()
            .expect("signing on a nonce stream that hasn't been initialized");

        let current = current_slot_value.index;
        let requested = index;

        if requested < current {
            return Err(NoncesUnavaillable::IndexUsed { current, requested });
        }

        if index.saturating_add(n) == u32::MAX {
            return Err(NoncesUnavaillable::Overflow);
        }

        Ok(())
    }

    pub fn sign_guaranteeing_nonces_destroyed(
        &mut self,
        session_id: SignSessionId,
        coord_nonce_state: CoordNonceStreamState,
        sessions: impl IntoIterator<Item = (PairedSecretShare<EvenY>, PartySignSession)>,
    ) -> (Vec<SignatureShare>, Option<NonceStreamSegment>) {
        let current_slot = self.current_slot();
        let current_slot_value = self.slots[current_slot]
            .read_slot()
            .expect("signing on a nonce stream that hasn't been initialized");

        if coord_nonce_state.index < current_slot_value.index {
            panic!("trying to sign with old nonce");
        }

        let (slot_with_sigs, slot_value) = match &current_slot_value.signing_state {
            Some(last_session) if last_session.session_id == session_id => {
                (current_slot, current_slot_value)
            }
            _ => {
                let next_slot = (current_slot + 1) % 2;
                let mut nonce_iter = current_slot_value
                    .iter_secret_nonces()
                    .skip((coord_nonce_state.index - current_slot_value.index) as usize);

                let mut signature_shares = vec![];
                let mut next_prg_state: Option<(u32, ChaChaSeed)> = None;

                for (secret_share, session) in sessions.into_iter() {
                    let (current_index, secret_nonce, next_seed) = nonce_iter
                        .next()
                        .expect("tried to sign with nonces out of range");
                    let next_index = current_index + 1;
                    next_prg_state = Some((next_index, next_seed));
                    let signature_share = session.sign(&secret_share, secret_nonce);
                    // TODO: verify the signature share as a sanity check
                    signature_shares.push(signature_share);
                }

                if signature_shares.is_empty() {
                    panic!("sign sessions must not be empty");
                }

                let (next_index, next_prg_seed) = next_prg_state.unwrap();

                let next_slot_value = SecretNonceSlot {
                    nonce_stream_id: current_slot_value.nonce_stream_id,
                    index: next_index,
                    ratchet_prg_seed: next_prg_seed,
                    signing_state: Some(SigningState {
                        session_id,
                        signature_shares,
                    }),
                };

                self.slots[next_slot].write_slot(&next_slot_value);
                (next_slot, next_slot_value)
            }
        };

        let other_slot = (slot_with_sigs + 1) % 2;
        self.slots[other_slot].write_slot(&slot_value);
        let signature_shares = slot_value
            .signing_state
            .expect("guaranteed")
            .signature_shares;

        let implied_coord_nonce_state = coord_nonce_state.after_signing(signature_shares.len());
        let replenishment = self.reconcile_coord_nonce_stream_state(implied_coord_nonce_state);

        (signature_shares, replenishment)
    }

    fn current_slot(&mut self) -> usize {
        if self.slots[1].read_index() > self.slots[0].read_index() {
            1
        } else {
            0
        }
    }

    pub fn current_index(&mut self) -> u32 {
        self.slots[0]
            .read_index()
            .max(self.slots[1].read_index())
            .unwrap_or(0)
    }

    pub fn nonce_stream_id(&mut self) -> Option<NonceStreamId> {
        self.slots[self.current_slot()]
            .read_slot()
            .map(|value| value.nonce_stream_id)
    }
}

impl SecretNonceSlot {
    fn iter_secret_nonces(&self) -> impl Iterator<Item = (u32, binonce::SecretNonce, ChaChaSeed)> {
        let mut prg_seed = self.ratchet_prg_seed;
        let mut index = self.index;

        core::iter::from_fn(move || {
            if index == u32::MAX {
                return None;
            }
            let mut chacha_nonce = [0u8; 12];
            let current_index = index;
            chacha_nonce[0..core::mem::size_of_val(&current_index)]
                .copy_from_slice(current_index.to_le_bytes().as_ref());
            let mut chacha = ChaCha20::new(&prg_seed.into(), &chacha_nonce.into());
            let mut next_seed = [0u8; 32];
            chacha.apply_keystream(&mut next_seed);
            let mut secret_nonce_bytes = [0u8; 64];
            chacha.apply_keystream(&mut secret_nonce_bytes);
            let secret_nonce = binonce::SecretNonce::from_bytes(secret_nonce_bytes)
                .expect("computationally unreachable");

            prg_seed = next_seed;
            index += 1;

            Some((current_index, secret_nonce, next_seed))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MemoryNonceSlot {
    inner: Option<SecretNonceSlot>,
}

impl NonceStreamSlot for MemoryNonceSlot {
    fn read_index(&mut self) -> Option<u32> {
        self.inner.as_ref().map(|slot| slot.index)
    }

    fn read_slot(&mut self) -> Option<SecretNonceSlot> {
        self.inner.clone()
    }

    fn write_slot(&mut self, value: &SecretNonceSlot) {
        self.inner = Some(value.clone())
    }
}

#[derive(Clone, Debug)]
pub enum NoncesUnavaillable {
    IndexUsed { current: u32, requested: u32 },
    Overflow,
}

impl core::fmt::Display for NoncesUnavaillable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NoncesUnavaillable::IndexUsed { current, requested } => {
                write!(
                    f,
                    "Attempt to reuse nonces! Current index: {current}. Requested: {requested}."
                )
            }
            NoncesUnavaillable::Overflow => {
                write!(f, "nonces were requested beyond the final index")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoncesUnavaillable {}
