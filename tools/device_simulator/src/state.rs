use bincode::{Decode, Encode};
use frostsnap_comms::BINCODE_CONFIG;
use frostsnap_core::Versioned;
use frostsnap_core::device::Mutation;
use frostsnap_core::device_nonces::{NonceStreamSlot, SecretNonceSlot};
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::prelude::*;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;

pub type SlotHandle = Arc<Mutex<Option<Versioned<SecretNonceSlot>>>>;
pub type LoadedSimState = (SimDeviceState, KeyPair, Vec<SlotHandle>);

pub const DEFAULT_NONCE_SLOTS: usize = 16;

#[derive(Clone, Debug)]
pub struct SharedNonceSlot(pub SlotHandle);

impl NonceStreamSlot for SharedNonceSlot {
    fn read_slot_versioned(&mut self) -> Option<Versioned<SecretNonceSlot>> {
        self.0.lock().clone()
    }

    fn write_slot_versioned(&mut self, value: Versioned<&SecretNonceSlot>) {
        *self.0.lock() = Some(value.cloned());
    }
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct SimDeviceState {
    pub keypair_secret: [u8; 32],
    pub name: Option<String>,
    pub mutations: Vec<Mutation>,
    pub nonce_slots: Vec<Option<Versioned<SecretNonceSlot>>>,
}

impl SimDeviceState {
    pub fn new(keypair_secret: [u8; 32], name: Option<String>) -> Self {
        Self {
            keypair_secret,
            name,
            mutations: Vec::new(),
            nonce_slots: vec![None; DEFAULT_NONCE_SLOTS],
        }
    }
}

pub fn save_state_atomic(path: &Path, state: &SimDeviceState) -> io::Result<()> {
    let encoded = bincode::encode_to_vec(state, BINCODE_CONFIG)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("sim_state"),
        std::process::id()
    ));

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp_path)?;

    file.write_all(&encoded)?;
    file.sync_all()?;
    drop(file);

    std::fs::rename(&temp_path, path)?;
    Ok(())
}

pub fn update_nonce_slots(state: &mut SimDeviceState, slot_handles: &[SlotHandle]) {
    state.nonce_slots = slot_handles.iter().map(|h| h.lock().clone()).collect();
}

pub fn load_or_create_state(path: &Path, cli_name: Option<String>) -> io::Result<LoadedSimState> {
    let mut state = if path.exists() {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let (loaded_state, _): (SimDeviceState, usize) =
            bincode::decode_from_slice(&data, BINCODE_CONFIG)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        loaded_state
    } else {
        let mut rng = rand::thread_rng();
        let scalar = Scalar::random(&mut rng);
        let secret_bytes = scalar.to_bytes();
        let new_state = SimDeviceState::new(secret_bytes, cli_name.clone());
        save_state_atomic(path, &new_state)?;
        new_state
    };

    if let Some(cli_name) = cli_name
        && state.name.as_ref() != Some(&cli_name)
    {
        state.name = Some(cli_name);
        save_state_atomic(path, &state)?;
    }

    let scalar = Scalar::from_slice_mod_order(&state.keypair_secret)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid scalar bytes"))?
        .non_zero()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "zero scalar"))?;
    let keypair = KeyPair::new(scalar);

    let mut slot_handles = Vec::new();
    for slot in &state.nonce_slots {
        slot_handles.push(Arc::new(Mutex::new(slot.clone())));
    }
    if slot_handles.is_empty() {
        for _ in 0..DEFAULT_NONCE_SLOTS {
            slot_handles.push(Arc::new(Mutex::new(None)));
        }
    }

    Ok((state, keypair, slot_handles))
}
