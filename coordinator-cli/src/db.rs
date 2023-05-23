use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use frostsnap_core::{CoordinatorFrostKey, DeviceId};

pub struct Db {
    path: PathBuf,
}

#[derive(bincode::Encode, bincode::Decode)]
pub struct State {
    #[bincode(with_serde)]
    pub key: CoordinatorFrostKey,
    #[bincode(with_serde)]
    pub device_labels: HashMap<DeviceId, String>,
}

impl Db {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> anyhow::Result<Option<State>> {
        let state = if self.path.exists() {
            let state_bytes = std::fs::read(&self.path)?;
            let (state, _): (State, _) =
                bincode::decode_from_slice(&state_bytes, bincode::config::standard())?;
            Some(state)
        } else {
            None
        };
        Ok(state)
    }

    pub fn save(&self, state: State) -> anyhow::Result<()> {
        std::fs::write(
            &self.path,
            bincode::encode_to_vec(state, bincode::config::standard()).unwrap(),
        )
        .context(format!("Unable to save to {}", self.path.display()))?;
        Ok(())
    }
}
