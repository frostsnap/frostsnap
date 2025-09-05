use alloc::collections::VecDeque;
use schnorr_fun::binonce;
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct NonceStreamSegment {
    pub stream_id: NonceStreamId,
    pub nonces: VecDeque<binonce::Nonce>,
    pub index: u32,
}

impl core::fmt::Debug for NonceStreamSegment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NonceStreamSegment")
            .field("stream_id", &self.stream_id)
            .field("index", &self.index)
            .field("first_nonce", &self.nonces.front())
            .field("nonce_count", &self.nonces.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SigningReqSubSegment {
    pub segment: NonceStreamSegment,
    pub remaining: u32,
}

impl SigningReqSubSegment {
    pub fn coord_nonce_state(&self) -> CoordNonceStreamState {
        CoordNonceStreamState {
            stream_id: self.segment.stream_id,
            index: self.segment.index,
            remaining: self.remaining,
        }
    }
}

impl NonceStreamSegment {
    /// When a coordinator needs to make a signing request you take a prefix of nonces from the
    /// segment. You also need to know the "remaining" nonces in the segment so you can tell the
    /// device about it so it replenishes at the right point.j
    pub fn signing_req_sub_segment(&self, length: usize) -> Option<SigningReqSubSegment> {
        let remaining = self.nonces.len().checked_sub(length)?.try_into().ok()?;

        let segment = Self {
            stream_id: self.stream_id,
            nonces: self.nonces.iter().cloned().take(length).collect(),
            index: self.index,
        };

        Some(SigningReqSubSegment { remaining, segment })
    }

    pub fn index_after_last(&self) -> Option<u32> {
        self.index.checked_add(self.nonces.len().try_into().ok()?)
    }

    pub fn delete_up_to(&mut self, up_to_but_not_including: u32) -> bool {
        let mut changed = false;
        let to_delete = up_to_but_not_including.saturating_sub(self.index);
        debug_assert!(to_delete as usize <= self.nonces.len());
        debug_assert!((to_delete as usize + self.index as usize) < u32::MAX as usize);
        for _ in 0..to_delete {
            self.nonces.pop_front();
            self.index += 1;
            changed = true;
        }
        changed
    }

    fn _extend(
        &self,
        other: NonceStreamSegment,
    ) -> Result<NonceStreamSegment, NonceSegmentIncompatible> {
        let mut curr = self.clone();
        if self.stream_id != other.stream_id {
            return Err(NonceSegmentIncompatible::StreamIdDontMatch);
        }
        other
            .index_after_last()
            .ok_or(NonceSegmentIncompatible::Overflows)?;

        let connect = if other.index > curr.index {
            curr.index + self.nonces.len() as u32 >= other.index
        } else {
            other.index + other.nonces.len() as u32 >= curr.index
        };

        if !connect || curr.nonces.is_empty() {
            return Ok(if curr.nonces.len() > other.nonces.len() {
                debug_assert!(false, "was unable to extend update");
                curr
            } else {
                other
            });
        }

        let new_start = self.index.min(other.index);
        let mut curr_end = self.index_after_last().expect("invariant") - 1;
        let other_end = other.index_after_last().unwrap() - 1;
        let new_end = curr_end.max(other_end);

        while curr.index > new_start {
            curr.index -= 1;
            let other_nonce = other.get_nonce(curr.index).expect("must exist");
            curr.nonces.push_front(other_nonce);
        }

        while curr_end < new_end {
            curr_end += 1;
            let other_nonce = other.get_nonce(curr_end).expect("must exist");
            curr.nonces.push_back(other_nonce);
        }

        assert_eq!(curr.index, new_start, "should start at the right place now");
        assert_eq!(
            curr.index_after_last().unwrap() - 1,
            new_end,
            "should end at the right place now"
        );

        Ok(curr)
    }

    pub fn extend(&mut self, other: NonceStreamSegment) -> Result<bool, NonceSegmentIncompatible> {
        let new = self._extend(other)?;
        if new != *self {
            *self = new;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn check_can_extend(
        &self,
        other: &NonceStreamSegment,
    ) -> Result<(), NonceSegmentIncompatible> {
        self._extend(other.clone())?;
        Ok(())
    }

    fn get_nonce(&self, index: u32) -> Option<binonce::Nonce> {
        self.nonces
            .get(index.checked_sub(self.index)? as usize)
            .copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NonceSegmentIncompatible {
    StreamIdDontMatch,
    Overflows,
}

impl core::fmt::Display for NonceSegmentIncompatible {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use NonceSegmentIncompatible::*;
        match self {
            StreamIdDontMatch => write!(f, "stream ids for nonce segments didn't match"),
            Overflows => write!(f, "the segment we are trying to connect overflows"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NonceSegmentIncompatible {}

/// A way to index nonces on a device.
/// Each device can produce a sequence of random nonces for any requested stream id
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct NonceStreamId(pub [u8; 16]);

impl NonceStreamId {
    pub fn random(rng: &mut impl rand_core::RngCore) -> Self {
        let mut bytes = [0u8; 16];
        rng.fill_bytes(&mut bytes);
        NonceStreamId(bytes)
    }
}

crate::impl_display_debug_serialize! {
    fn to_bytes(nonce_stream_id: &NonceStreamId) -> [u8;16] {
        nonce_stream_id.0
    }
}

crate::impl_fromstr_deserialize! {
    name => "nonce stream id",
    fn from_bytes(bytes: [u8;16]) -> NonceStreamId {
        NonceStreamId(bytes)
    }
}

#[derive(Debug, Clone, Copy, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordNonceStreamState {
    pub stream_id: NonceStreamId,
    pub index: u32,
    pub remaining: u32,
}

impl CoordNonceStreamState {
    pub fn after_signing(mut self, n_sigs: usize) -> Self {
        self.index = self.index.checked_add(n_sigs as u32).unwrap();
        self.remaining = self.remaining.checked_sub(n_sigs as u32).unwrap();
        self
    }
}
