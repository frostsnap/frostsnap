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
    SignSessionId, Versioned, NONCE_BATCH_SIZE,
};

type ChaChaSeed = [u8; 32];

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SecretNonceSlot {
    pub index: u32,
    pub nonce_stream_id: NonceStreamId,
    pub ratchet_prg_seed: ChaChaSeed,
    /// for clearing slots based on least recently used
    pub last_used: u32,
    pub signing_state: Option<SigningState>,
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SigningState {
    pub session_id: SignSessionId,
    pub signature_shares: Vec<SignatureShare>,
}

pub trait NonceStreamSlot {
    fn read_slot_versioned(&mut self) -> Option<Versioned<SecretNonceSlot>>;
    fn write_slot_versioned(&mut self, value: Versioned<&SecretNonceSlot>);
    fn read_slot(&mut self) -> Option<SecretNonceSlot> {
        match self.read_slot_versioned()? {
            Versioned::V0(v) => Some(v),
        }
    }

    fn write_slot(&mut self, value: &SecretNonceSlot) {
        self.write_slot_versioned(Versioned::V0(value))
    }

    fn initialize(&mut self, stream_id: NonceStreamId, last_used: u32, rng: &mut impl RngCore) {
        let mut ratchet_prg_seed = ChaChaSeed::default();
        rng.fill_bytes(&mut ratchet_prg_seed[..]);
        let value = SecretNonceSlot {
            index: 0,
            nonce_stream_id: stream_id,
            ratchet_prg_seed,
            last_used,
            signing_state: None,
        };
        self.write_slot(&value);
    }

    fn reconcile_coord_nonce_stream_state(
        &mut self,
        state: CoordNonceStreamState,
    ) -> Option<NonceStreamSegment> {
        let value = self.read_slot()?;
        let our_index = value.index;
        if our_index > state.index || state.remaining < NONCE_BATCH_SIZE {
            Some(value.nonce_segment(None, NONCE_BATCH_SIZE as usize))
        } else if our_index == state.index {
            None
        } else {
            Some(value.nonce_segment(None, (state.index - our_index) as usize))
        }
    }

    fn nonce_stream_id(&mut self) -> Option<NonceStreamId> {
        self.read_slot().map(|value| value.nonce_stream_id)
    }

    fn sign_guaranteeing_nonces_destroyed(
        &mut self,
        session_id: SignSessionId,
        coord_nonce_state: CoordNonceStreamState,
        last_used: u32,
        sessions: impl IntoIterator<Item = (PairedSecretShare<EvenY>, PartySignSession)>,
    ) -> (Vec<SignatureShare>, Option<NonceStreamSegment>) {
        let slot_value = self
            .read_slot()
            .expect("cannot sign with uninitialized slot");
        assert_eq!(
            coord_nonce_state.stream_id, slot_value.nonce_stream_id,
            "wrong stream id"
        );

        if coord_nonce_state.index < slot_value.index {
            panic!("trying to sign with old nonce");
        }

        let with_signatures = match &slot_value.signing_state {
            Some(SigningState {
                session_id: saved_session_id,
                ..
            }) if *saved_session_id == session_id => {
                // We've already got the signatures for this session on flash so we don't need to
                // sign. But this doesn't mean we don't need to write it to flash again. We don't
                // know that the previous state was erased. So we may redundantly rewrite it but
                // that's ok.
                slot_value
            }
            _ => {
                let mut nonce_iter = slot_value
                    .iter_secret_nonces()
                    .skip((coord_nonce_state.index - slot_value.index) as usize);

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

                SecretNonceSlot {
                    nonce_stream_id: slot_value.nonce_stream_id,
                    index: next_index,
                    last_used,
                    ratchet_prg_seed: next_prg_seed,
                    signing_state: Some(SigningState {
                        session_id,
                        signature_shares,
                    }),
                }
            }
        };

        self.write_slot(&with_signatures);

        // XXX Read the slot back in to be 100% certain it was written
        let signature_shares = self
            .read_slot()
            .expect("guaranteed")
            .signing_state
            .expect("guaranteed")
            .signature_shares;

        let implied_coord_nonce_state = coord_nonce_state.after_signing(signature_shares.len());
        let replenishment = self.reconcile_coord_nonce_stream_state(implied_coord_nonce_state);

        (signature_shares, replenishment)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AbSlots<S> {
    slots: Vec<S>,
    last_used: u32,
}

impl<S: NonceStreamSlot> AbSlots<S> {
    pub fn new(mut slots: Vec<S>) -> Self {
        let last_used = slots
            .iter_mut()
            .filter_map(|slot| slot.read_slot().map(|v| v.last_used))
            .max()
            .unwrap_or(0);
        Self {
            slots: slots.into_iter().collect(),
            last_used,
        }
    }

    pub fn sign_guaranteeing_nonces_destroyed(
        &mut self,
        session_id: SignSessionId,
        coord_nonce_state: CoordNonceStreamState,
        sessions: impl IntoIterator<Item = (PairedSecretShare<EvenY>, PartySignSession)>,
    ) -> Option<(Vec<SignatureShare>, Option<NonceStreamSegment>)> {
        let last_used = self.last_used + 1;
        let slot = self.get(coord_nonce_state.stream_id)?;
        let out = slot.sign_guaranteeing_nonces_destroyed(
            session_id,
            coord_nonce_state,
            last_used,
            sessions,
        );
        self.last_used = last_used;
        Some(out)
    }

    fn increment_last_used(&mut self) -> u32 {
        self.last_used += 1;
        self.last_used
    }

    pub fn get_or_create(&mut self, stream_id: NonceStreamId, rng: &mut impl RngCore) -> &mut S {
        // the algorithm is to find the first empty slot or to choose the one with the lowest `last_used`
        let mut i = 0;
        let mut lowest_last_used = u32::MAX;
        let mut idx_lowest_last_used = 0;
        let last_used = self.increment_last_used();
        let found = loop {
            if i >= self.slots.len() {
                break None;
            }
            let ab_slot = &mut self.slots[i];
            let value = ab_slot.read_slot();
            match value {
                Some(value) => {
                    if value.nonce_stream_id == stream_id {
                        break Some(i);
                    } else if value.last_used < lowest_last_used {
                        idx_lowest_last_used = i;
                        lowest_last_used = value.last_used;
                    }
                }
                None => {
                    ab_slot.initialize(stream_id, last_used, rng);
                    break Some(i);
                }
            }
            i += 1;
        };

        match found {
            Some(i) => &mut self.slots[i],
            None => {
                let ab_slot = &mut self.slots[idx_lowest_last_used];
                ab_slot.initialize(stream_id, last_used, rng);
                ab_slot
            }
        }
    }

    pub fn get(&mut self, stream_id: NonceStreamId) -> Option<&mut S> {
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

    pub fn total_slots(&self) -> usize {
        self.slots.len()
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

    pub fn are_nonces_available(&self, index: u32, n: u32) -> Result<(), NoncesUnavaillable> {
        let current = self.index;
        let requested = index;

        if requested < current {
            return Err(NoncesUnavaillable::IndexUsed { current, requested });
        }

        if index.saturating_add(n) == u32::MAX {
            return Err(NoncesUnavaillable::Overflow);
        }

        Ok(())
    }

    pub fn nonce_segment(&self, start: Option<u32>, length: usize) -> NonceStreamSegment {
        let start = start.unwrap_or(self.index);

        if start < self.index {
            panic!("can't iterate erased nonces");
        }
        let last = start.saturating_add(length.try_into().expect("length not too big"));

        if last == u32::MAX {
            panic!("cannot have an index at u32::MAX");
        }

        let nonces = self
            .iter_pub_nonces()
            .skip((start - self.index) as _)
            .map(|(_, nonce)| nonce)
            .take(length)
            .collect::<VecDeque<_>>();

        assert_eq!(nonces.len(), length, "there weren't enough noces for that");

        NonceStreamSegment {
            stream_id: self.nonce_stream_id,
            index: start,
            nonces,
        }
    }

    pub fn iter_pub_nonces(&self) -> impl Iterator<Item = (u32, binonce::Nonce)> {
        self.iter_secret_nonces().map(|(index, secret_nonce, _)| {
            (index, NonceKeyPair::from_secret(secret_nonce).public())
        })
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MemoryNonceSlot {
    inner: Option<Versioned<SecretNonceSlot>>,
}

impl NonceStreamSlot for MemoryNonceSlot {
    fn read_slot_versioned(&mut self) -> Option<Versioned<SecretNonceSlot>> {
        self.inner.clone()
    }

    fn write_slot_versioned(&mut self, value: Versioned<&SecretNonceSlot>) {
        self.inner = Some(value.cloned())
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
