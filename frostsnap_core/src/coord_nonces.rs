use crate::{nonce_stream::*, DeviceId};
use alloc::collections::*;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct NonceCache {
    by_device: BTreeMap<DeviceId, BTreeMap<NonceStreamId, NonceStreamSegment>>,
}

impl NonceCache {
    pub fn extend_segment(
        &mut self,
        device_id: DeviceId,
        new_segment: NonceStreamSegment,
    ) -> Result<bool, NonceSegmentIncompatible> {
        let nonce_segments = self.by_device.entry(device_id).or_default();

        let segment = nonce_segments
            .entry(new_segment.stream_id)
            .or_insert(NonceStreamSegment {
                stream_id: new_segment.stream_id,
                nonces: Default::default(),
                index: 0,
            });

        segment.extend(new_segment)
    }

    pub fn check_can_extend(
        &self,
        device_id: DeviceId,
        new_segment: &NonceStreamSegment,
    ) -> Result<(), NonceSegmentIncompatible> {
        match self.by_device.get(&device_id) {
            Some(device_segments) => match device_segments.get(&new_segment.stream_id) {
                Some(segment) => segment.check_can_extend(new_segment),
                None => Ok(()),
            },
            None => Ok(()),
        }
    }

    pub fn new_signing_session(
        &mut self,
        devices: &BTreeSet<DeviceId>,
        n_nonces: usize,
        used_streams: &BTreeSet<NonceStreamId>,
    ) -> Result<BTreeMap<DeviceId, SigningReqSubSegment>, NotEnoughNonces> {
        let mut nonces_chosen: BTreeMap<DeviceId, SigningReqSubSegment> = BTreeMap::default();

        for &device in devices {
            let by_device = self.by_device.entry(device).or_default();
            let mut nonces_available = 0;
            for (stream_id, stream_segment) in by_device {
                if used_streams.contains(stream_id) {
                    continue;
                }
                if stream_segment.index_after_last().is_none() {
                    continue;
                }
                if let Some(sub_segment) = stream_segment.signing_req_sub_segment(n_nonces) {
                    nonces_chosen.insert(device, sub_segment);
                    break;
                } else {
                    nonces_available = nonces_available.max(stream_segment.nonces.len());
                }
            }

            if !nonces_chosen.contains_key(&device) {
                return Err(NotEnoughNonces {
                    device_id: device,
                    available: nonces_available,
                    need: n_nonces,
                });
            }
        }

        Ok(nonces_chosen)
    }

    pub fn consume(
        &mut self,
        device_id: DeviceId,
        stream_id: NonceStreamId,
        up_to_but_not_including: u32,
    ) -> bool {
        if let Some(device_streams) = self.by_device.get_mut(&device_id) {
            if let Some(local_segment) = device_streams.get_mut(&stream_id) {
                assert!(
                    local_segment.index <= up_to_but_not_including,
                    "tried to consume no nonces since counter {} was greater than consumption point {}",
                    local_segment.index,
                    up_to_but_not_including
                );

                return local_segment.delete_up_to(up_to_but_not_including);
            }
        }
        false
    }

    pub fn nonces_available(
        &self,
        device_id: DeviceId,
        used_streams: &BTreeSet<NonceStreamId>,
    ) -> BTreeMap<NonceStreamId, u32> {
        let mut available = BTreeMap::default();
        if let Some(streams) = self.by_device.get(&device_id) {
            for (stream_id, stream) in streams {
                if used_streams.contains(stream_id) {
                    continue;
                }
                if !stream.nonces.is_empty() {
                    available.insert(*stream_id, stream.nonces.len() as u32);
                }
            }
        }

        available
    }

    pub fn generate_nonce_stream_opening_requests(
        &self,
        device_id: DeviceId,
        min_streams: usize,
        rng: &mut impl rand_core::RngCore,
    ) -> impl IntoIterator<Item = CoordNonceStreamState> {
        let mut stream_ids = vec![];
        let streams = self.by_device.get(&device_id).cloned().unwrap_or_default();
        let new_streams_needed = min_streams.saturating_sub(streams.len());
        for _ in 0..new_streams_needed {
            stream_ids.push(CoordNonceStreamState {
                stream_id: NonceStreamId::random(rng),
                index: 0,
                remaining: 0,
            });
        }

        for (stream_id, stream) in streams {
            stream_ids.push(CoordNonceStreamState {
                stream_id,
                index: stream.index,
                remaining: stream.nonces.len().try_into().unwrap(),
            })
        }

        stream_ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotEnoughNonces {
    device_id: DeviceId,
    available: usize,
    need: usize,
}

impl core::fmt::Display for NotEnoughNonces {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let NotEnoughNonces {
            device_id,
            available,
            need,
        } = self;
        write!(f, "coordinator doesn't have enough nonces for {device_id}. It only has {available} but needs {need}")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NotEnoughNonces {}
