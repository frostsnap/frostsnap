use bdk_file_store::Store;
use frostsnap_core::{bincode, CoordinatorFrostKey, DeviceId};
use std::{collections::HashMap, path::PathBuf};

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChangeSet {
    pub frostsnap: Option<State>,
    pub wallet: crate::wallet::ChangeSet,
}

impl From<crate::wallet::ChangeSet> for ChangeSet {
    fn from(value: crate::wallet::ChangeSet) -> Self {
        Self {
            wallet: value,
            ..Default::default()
        }
    }
}

impl bdk_chain::Append for ChangeSet {
    fn append(&mut self, other: ChangeSet) {
        if other.frostsnap.is_some() {
            self.frostsnap = other.frostsnap;
        }
        self.wallet.append(other.wallet);
    }

    fn is_empty(&self) -> bool {
        self.frostsnap.is_none() && self.wallet.is_empty()
    }
}

impl From<State> for ChangeSet {
    fn from(value: State) -> Self {
        ChangeSet {
            frostsnap: Some(value),
            ..Default::default()
        }
    }
}

pub static FILE_MAGIC_BYTES: &[u8] = "🥶❄❆❅🥶".as_bytes();

pub struct Db {
    store: Store<'static, ChangeSet>,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub key: CoordinatorFrostKey,
    pub device_labels: HashMap<DeviceId, String>,
}

impl Db {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        let store = Store::new_from_path(FILE_MAGIC_BYTES, &path)?;
        Ok(Self { store })
    }

    pub fn load(&mut self) -> anyhow::Result<ChangeSet> {
        let (changeset, res) = self.store.aggregate_changesets();
        res?;
        Ok(changeset)
    }

    pub fn save<C: Into<ChangeSet>>(&mut self, changeset: C) -> anyhow::Result<()> {
        Ok(self.store.append_changeset(&changeset.into())?)
    }
}
