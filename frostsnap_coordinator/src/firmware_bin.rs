use frostsnap_comms::{Sha256Digest, FIRMWARE_UPGRADE_CHUNK_LEN};

#[derive(Clone, Copy)]
pub struct FirmwareBin {
    bin: &'static [u8],
    digest_cache: Option<Sha256Digest>,
}

impl frostsnap_comms::firmware_reader::FirmwareReader for FirmwareBin {
    type Error = std::io::Error;

    fn read_sector(
        &self,
        sector: u32,
    ) -> Result<Box<[u8; frostsnap_comms::firmware_reader::SECTOR_SIZE]>, Self::Error> {
        use frostsnap_comms::firmware_reader::SECTOR_SIZE;

        let sector_offset = (sector as usize) * SECTOR_SIZE;
        if sector_offset >= self.bin.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Sector out of bounds",
            ));
        }

        let mut sector_data = Box::new([0u8; SECTOR_SIZE]);
        let end = (sector_offset + SECTOR_SIZE).min(self.bin.len());
        let data_len = end - sector_offset;
        sector_data[..data_len].copy_from_slice(&self.bin[sector_offset..end]);

        Ok(sector_data)
    }

    fn n_sectors(&self) -> u32 {
        use frostsnap_comms::firmware_reader::SECTOR_SIZE;
        self.bin.len().div_ceil(SECTOR_SIZE) as u32
    }
}

impl FirmwareBin {
    pub const fn is_stub(&self) -> bool {
        self.bin.is_empty()
    }

    pub const fn new(bin: &'static [u8]) -> Self {
        Self {
            bin,
            digest_cache: None,
        }
    }

    pub fn num_chunks(&self) -> u32 {
        (self.bin.len() as u32).div_ceil(FIRMWARE_UPGRADE_CHUNK_LEN)
    }

    pub fn size(&self) -> u32 {
        self.bin.len() as u32
    }

    pub fn as_bytes(&self) -> &'static [u8] {
        self.bin
    }

    pub fn cached_digest(&mut self) -> Sha256Digest {
        let digest_cache = self.digest_cache.take();
        let digest = digest_cache.unwrap_or_else(|| self.digest());
        self.digest_cache = Some(digest);
        digest
    }

    pub fn digest(&self) -> Sha256Digest {
        use frostsnap_core::sha2::digest::Digest;
        let mut state = sha2::Sha256::default();
        state.update(self.bin);
        Sha256Digest(state.finalize().into())
    }

    pub fn find_signature_block(&self) -> Option<usize> {
        use frostsnap_comms::{SIGNATURE_BLOCK_MAGIC, SIGNATURE_BLOCK_SIZE};

        if self.bin.len() < SIGNATURE_BLOCK_SIZE {
            return None;
        }

        let potential_sig_start = self.bin.len() - SIGNATURE_BLOCK_SIZE;
        if self.bin[potential_sig_start..].starts_with(&SIGNATURE_BLOCK_MAGIC) {
            Some(potential_sig_start)
        } else {
            None
        }
    }

    pub fn firmware_only_digest(&self) -> Sha256Digest {
        use frostsnap_core::sha2::digest::Digest;

        match frostsnap_comms::firmware_reader::firmware_size(self) {
            Ok((firmware_only_size, _total_size)) => {
                let mut state = sha2::Sha256::default();
                state.update(&self.bin[..firmware_only_size as usize]);
                Sha256Digest(state.finalize().into())
            }
            Err(_) => {
                let firmware_only = match self.find_signature_block() {
                    Some(sig_start) => &self.bin[..sig_start],
                    None => self.bin,
                };
                let mut state = sha2::Sha256::default();
                state.update(firmware_only);
                Sha256Digest(state.finalize().into())
            }
        }
    }
}
