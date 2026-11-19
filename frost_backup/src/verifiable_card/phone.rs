//! The phone's side, with owned types it can keep in its records: it gathers the seal's points
//! frame by frame, pins the seal with the phone half P the device sent, verifies the commit
//! proofs as they arrive, checks every later P against the pin, and gathers and checks each
//! opening. Everything here takes its input from the device, so malformed frames are refused
//! with an error, never a panic.
//!
//! The pin keeps ctx and h_C, never P: P lives in the phone's RAM only (design O5) and is
//! pulled again after an app restart, and every P, first or later, must hash to the hP inside
//! the pinned h_C (binding (1), PK.6).

use super::{
    layout::{Bits, CARRIES_PER_BOX, COMMITTED, HALF, PHONE_HALF, WORD_BITS},
    proof::{
        phone_half_matches, Context, Opening, Ops, Proven, Seal, Setup, Tagged, VerifyError,
        CHECK_BOXES, OPENING_SPLIT, POINTS_PER_FRAME, POINT_LEN, PROOFS_PER_FRAME, PROOF_LEN,
    },
};
use alloc::vec::Vec;

// DRAFT: frozen before the first release (spec PK-O3).
/// The tag of [`phone_words_hash`].
pub const PHONE_WORDS_TAG: &[u8] = b"frostsnap/card/phone-words-v1";

/// The range `from..from + len` of committed bits, if it's a frame's worth inside 0..311.
fn frame_range(from: u16, len: usize, max: usize) -> Result<core::ops::Range<usize>, VerifyError> {
    let from = from as usize;
    match from.checked_add(len) {
        Some(end) if (1..=max).contains(&len) && end <= COMMITTED => Ok(from..end),
        _ => Err(VerifyError::Frame),
    }
}

/// The first `v` not in `done`, and how many in a row from it aren't, up to `max`.
fn first_missing(done: &Bits, max: usize) -> Option<(u16, u8)> {
    let from = (0..COMMITTED).find(|&v| !done.get(v))?;
    let count = (from..COMMITTED)
        .take(max)
        .take_while(|&v| !done.get(v))
        .count();
    Some((from as u16, count as u8))
}

/// The seal's points, gathered from `CommitPoints` frames in any order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SealBuilder {
    /// ser(C_v), with zeros where nothing has arrived yet.
    points: Vec<[u8; POINT_LEN]>,
    received: Bits,
}

impl SealBuilder {
    /// Takes 1 to 32 points starting at C_from. It refuses a frame past C_310 or one that
    /// overlaps points already taken.
    pub fn add(&mut self, from: u16, points: &[[u8; POINT_LEN]]) -> Result<(), VerifyError> {
        let range = frame_range(from, points.len(), POINTS_PER_FRAME)?;
        if range.clone().any(|v| self.received.get(v)) {
            return Err(VerifyError::Frame);
        }
        self.points.resize(COMMITTED, [0; POINT_LEN]);
        for (v, point) in range.zip(points) {
            self.points[v] = *point;
            self.received.set(v, true);
        }
        Ok(())
    }

    /// The next frame to pull: the first missing C and how many in a row are missing, up to 32.
    pub fn next_missing(&self) -> Option<(u16, u8)> {
        first_missing(&self.received, POINTS_PER_FRAME)
    }

    /// The seal, once every C has arrived. It's checked only when it's pinned.
    pub fn finish(self, coin_commit: [u8; 32], h_c: [u8; 32]) -> Result<Seal, VerifyError> {
        if self.received.len() != COMMITTED {
            return Err(VerifyError::Shape);
        }
        Ok(Seal {
            commitments: self.points,
            coin_commit,
            h_c,
        })
    }
}

/// A seal the phone has pinned, and which commit proofs have verified. It's [`Proven`] once
/// all 311 have. It holds no P: [`check_phone_half`](Self::check_phone_half) checks any P
/// against it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedSeal {
    seal: Seal,
    /// The ctx of the context it was checked under.
    ctx: [u8; 32],
    /// The index of the context it was checked under.
    index: u8,
    verified: Bits,
}

impl PinnedSeal {
    /// A seal from the phone's own record, with these proofs verified when it was recorded,
    /// under the phone's own context. Its points are checked again by nothing, so only the
    /// phone's own record may give it; any P is still checked against its h_C.
    pub fn from_record(seal: Seal, context: &Context, verified: Bits) -> Self {
        Self {
            seal,
            ctx: context.hash(&mut Ops::default()),
            index: context.meta.index,
            verified,
        }
    }

    /// Whether this phone half is the one the seal was pinned with (binding (1), PK.6): no word
    /// is 2048 or more ([`VerifyError::WordRange`]), and h_C over the pinned ctx and hP(P) is
    /// the pinned h_C ([`VerifyError::PhoneHalf`]). The phone runs it on every P it takes after
    /// the pin: a re-pull after an app restart, a second phone, a later check. It gives hP.
    pub fn check_phone_half(
        &self,
        phone_half: &[u16; HALF],
        ops: &mut Ops,
    ) -> Result<[u8; 32], VerifyError> {
        phone_half_matches(&self.seal, &self.ctx, phone_half, ops)
    }

    pub fn seal(&self) -> &Seal {
        &self.seal
    }

    /// The C's whose bit proofs have verified.
    pub fn verified(&self) -> &Bits {
        &self.verified
    }

    /// The next proofs to pull: the first unverified C and how many in a row are unverified,
    /// up to `max` (at least 1).
    pub fn next_missing(&self, max: u8) -> Option<(u16, u8)> {
        first_missing(&self.verified, max.max(1) as usize)
    }

    /// The seal, once every commit proof has verified.
    pub fn proven(&self) -> Option<Proven<'_>> {
        ((0..COMMITTED).all(|v| self.verified.get(v)) && self.seal.commitments.len() == COMMITTED)
            .then(|| Proven::new(&self.seal, self.ctx, self.index))
    }
}

impl Setup {
    /// Pins a seal with the phone half the device sent (spec §7.3.1 (2), PK.5): P has no word
    /// of 2048 or more, every C_v parses and isn't the identity or G, Σ_t 2^t·C_t = S_i, and h_C
    /// over the phone's own ctx and hP(P) is the seal's. It keeps ctx, not P.
    pub fn pin(
        &self,
        context: &Context,
        seal: Seal,
        phone_half: &[u16; HALF],
        ops: &mut Ops,
    ) -> Result<PinnedSeal, VerifyError> {
        let ctx = self.check_seal_points(context, &seal, phone_half, ops)?;
        Ok(PinnedSeal {
            seal,
            ctx,
            index: context.meta.index,
            verified: Bits::default(),
        })
    }

    /// Stock verification of C_v's bit proof against the pinned seal, noting it in `pinned`
    /// when it passes. [`verify_commit_proofs`](Self::verify_commit_proofs) does a frame.
    #[must_use]
    pub fn verify_commit_bit(
        &self,
        pinned: &mut PinnedSeal,
        v: usize,
        proof: &[u8; PROOF_LEN],
        ops: &mut Ops,
    ) -> bool {
        let passed = self.commit_proof_holds(&pinned.seal, v, proof, ops);
        if passed {
            pinned.verified.set(v, true);
        }
        passed
    }

    /// Verifies a `BitProofs` frame, 1 to 16 proofs starting at C_from's, and notes them in
    /// `pinned` only if every one passes. Proofs already verified may come again.
    pub fn verify_commit_proofs(
        &self,
        pinned: &mut PinnedSeal,
        from: u16,
        proofs: &[[u8; PROOF_LEN]],
        ops: &mut Ops,
    ) -> Result<(), VerifyError> {
        let range = frame_range(from, proofs.len(), PROOFS_PER_FRAME)?;
        for (v, proof) in range.clone().zip(proofs) {
            if !self.commit_proof_holds(&pinned.seal, v, proof, ops) {
                return Err(VerifyError::CommitProof(v));
            }
        }
        for v in range {
            pinned.verified.set(v, true);
        }
        Ok(())
    }

    /// The phone's check of an opening of word `j + 1`, which must be in D, against the phone
    /// half P the phone holds: everything [`verify_opening`](Self::verify_opening) checks,
    /// including that P hashes to the hP inside the proven h_C. Every word of D is a computed
    /// device word, so every one takes this check. It gives the opened word.
    #[allow(clippy::too_many_arguments)]
    pub fn check_opening(
        &self,
        proven: &Proven<'_>,
        phone_half: &[u16; HALF],
        n: &[u8; 32],
        d: &[u8; CHECK_BOXES],
        j: u8,
        opening: &Opening,
        ops: &mut Ops,
    ) -> Result<u16, VerifyError> {
        if !d.contains(&j) {
            return Err(VerifyError::NotInD);
        }
        self.verify_opening(proven, phone_half, n, j as usize, opening, ops)?;
        Ok(opening.y)
    }
}

/// TH("frostsnap/card/phone-words-v1", h_C ‖ u8(len) ‖ (u8 j ‖ u16be y)*), over the entries in
/// the order given. Use [`h32`] and [`h37`], which pin the order.
///
/// # Panics
///
/// With fewer than 32 entries: a hash over fewer words could be brute-forced.
pub fn phone_words_hash(h_c: &[u8; 32], words: &[(u8, u16)]) -> [u8; 32] {
    assert!(words.len() >= HALF, "a hash over fewer than 32 words");
    let mut hash = Tagged::new(PHONE_WORDS_TAG)
        .add(h_c)
        .add(&[words.len() as u8]);
    for (j, y) in words {
        hash = hash.add(&[*j]).add(&y.to_be_bytes());
    }
    hash.finish(&mut Ops::default())
}

/// h32: words 33-64, j ascending.
pub fn h32(h_c: &[u8; 32], phone_half: &[u16; HALF]) -> [u8; 32] {
    phone_words_hash(h_c, &phone_entries(phone_half))
}

/// h37: words 33-64, j ascending, then the 5 words of D in `derive_boxes` order (the order the
/// phone round asks them).
pub fn h37(
    h_c: &[u8; 32],
    phone_half: &[u16; HALF],
    d: &[u8; CHECK_BOXES],
    d_words: &[u16; CHECK_BOXES],
) -> [u8; 32] {
    let mut entries = phone_entries(phone_half);
    entries.extend(d.iter().copied().zip(d_words.iter().copied()));
    let hash = phone_words_hash(h_c, &entries);
    entries.iter_mut().for_each(|entry| *entry = (0, 0));
    hash
}

fn phone_entries(phone_half: &[u16; HALF]) -> Vec<(u8, u16)> {
    PHONE_HALF
        .zip(phone_half.iter().copied())
        .map(|(j, y)| (j as u8, y))
        .collect()
}

/// An opening, gathered from its three `OpeningPart` frames in order: the word and the Q's,
/// then the first 39 proofs, then the other 38.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningBuilder {
    j: u8,
    carries: Option<(u16, Vec<[u8; POINT_LEN]>)>,
    proofs: Vec<[u8; PROOF_LEN]>,
}

impl OpeningBuilder {
    /// For the box the phone asked for.
    pub fn new(j: u8) -> Self {
        Self {
            j,
            carries: None,
            proofs: Vec::new(),
        }
    }

    /// Part 0: the word and the 77 Q's. A box other than the one asked for is `WrongBox` and a
    /// word of 2048 or more `WordRange`, before anything else is looked at.
    pub fn add_carries(
        &mut self,
        j: u8,
        y: u16,
        carries: &[[u8; POINT_LEN]],
    ) -> Result<(), VerifyError> {
        if j != self.j {
            return Err(VerifyError::WrongBox);
        }
        if y >= 1 << WORD_BITS {
            return Err(VerifyError::WordRange);
        }
        if self.carries.is_some() || carries.len() != CARRIES_PER_BOX {
            return Err(VerifyError::Frame);
        }
        self.carries = Some((y, carries.to_vec()));
        Ok(())
    }

    /// Parts 1 and 2: the proofs from `first`, after the Q's and in order.
    pub fn add_proofs(
        &mut self,
        j: u8,
        first: u8,
        proofs: &[[u8; PROOF_LEN]],
    ) -> Result<(), VerifyError> {
        if j != self.j {
            return Err(VerifyError::WrongBox);
        }
        let expected = match self.proofs.len() {
            0 => OPENING_SPLIT,
            OPENING_SPLIT => CARRIES_PER_BOX - OPENING_SPLIT,
            _ => return Err(VerifyError::Frame),
        };
        if self.carries.is_none() || first as usize != self.proofs.len() || proofs.len() != expected
        {
            return Err(VerifyError::Frame);
        }
        self.proofs.extend_from_slice(proofs);
        Ok(())
    }

    /// Whether all three parts are in.
    pub fn is_complete(&self) -> bool {
        self.carries.is_some() && self.proofs.len() == CARRIES_PER_BOX
    }

    /// The opening, once all three parts are in. It's unchecked until
    /// [`Setup::check_opening`].
    pub fn finish(self) -> Option<Opening> {
        let (y, carries) = self.carries?;
        (self.proofs.len() == CARRIES_PER_BOX).then_some(Opening {
            j: self.j,
            y,
            carries,
            proofs: self.proofs,
        })
    }
}

impl Opening {
    /// Part 0: j, the word and the Q's.
    pub fn carries_part(&self) -> (u8, u16, &[[u8; POINT_LEN]]) {
        (self.j, self.y, &self.carries)
    }

    /// Part 1 (the first 39 proofs) or part 2 (the rest), with the index of its first proof.
    pub fn proofs_part(&self, part: u8) -> Option<(u8, &[[u8; PROOF_LEN]])> {
        let range = match part {
            1 => 0..OPENING_SPLIT,
            2 => OPENING_SPLIT..CARRIES_PER_BOX,
            _ => return None,
        };
        Some((range.start as u8, self.proofs.get(range)?))
    }
}
