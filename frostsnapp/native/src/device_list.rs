use crate::api::{self, ConnectedDevice, DeviceListState};
use frostsnap_coordinator::{frostsnap_core::DeviceId, DeviceChange};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    devices: Vec<DeviceId>,
    connected: HashMap<DeviceId, api::ConnectedDevice>,
    state_counter: usize,
    outbox: Vec<crate::api::DeviceListChange>,
}

impl DeviceList {
    pub fn update_ready(&self) -> bool {
        !self.outbox.is_empty()
    }

    pub fn devices(&self) -> Vec<ConnectedDevice> {
        self.devices
            .iter()
            .cloned()
            .map(|id| self.connected.get(&id).cloned().expect("invariant"))
            .collect()
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::ConnectedDevice> {
        self.devices
            .get(index)
            .map(|id| self.connected.get(id).cloned().expect("invariant"))
    }

    pub fn take_update(&mut self) -> api::DeviceListUpdate {
        let changes = core::mem::take(&mut self.outbox);
        let update = api::DeviceListUpdate {
            changes,
            state: DeviceListState {
                devices: self.devices(),
                state_id: self.state_counter,
            },
        };

        if !update.changes.is_empty() {
            self.state_counter += 1;
        }

        update
    }

    pub fn consume_manager_event(&mut self, change: api::DeviceChange) {
        match change {
            DeviceChange::Connected {
                id,
                firmware_digest,
                latest_firmware_digest,
                model,
            } => {
                let connected = self.connected.entry(id).or_insert(api::ConnectedDevice {
                    firmware_digest: Default::default(),
                    latest_digest: Default::default(),
                    name: None,
                    id,
                    model,
                    recovery_mode: false,
                });

                connected.firmware_digest = firmware_digest.to_string();
                connected.latest_digest = latest_firmware_digest.map(|digest| digest.to_string());
            }
            DeviceChange::NameChange { id, name } => {
                if let Some(index) = self.index_of(id) {
                    let connected = self.connected.get_mut(&id).expect("invariant");
                    connected.name = Some(name);
                    self.outbox.push(api::DeviceListChange {
                        kind: api::DeviceListChangeKind::Named,
                        index,
                        device: connected.clone(),
                    });
                }
            }
            DeviceChange::NeedsName { id } => {
                self.append(id);
            }
            DeviceChange::Registered { id, name } => {
                let index = self.index_of(id);
                let connected = self
                    .connected
                    .get_mut(&id)
                    .expect("registered means connected already emitted");
                match index {
                    Some(index) => {
                        if connected.name.is_some() {
                            assert_eq!(
                                connected.name,
                                Some(name.clone()),
                                "we should have got a renamed event if they were different"
                            );
                        } else {
                            // The device had no name and now it's been named
                            connected.name = Some(name);
                            self.outbox.push(api::DeviceListChange {
                                kind: api::DeviceListChangeKind::Named,
                                index,
                                device: connected.clone(),
                            });
                        }
                    }
                    None => {
                        connected.name = Some(name);
                        self.append(id);
                    }
                }
            }
            DeviceChange::Disconnected { id } => {
                let device = self.connected.remove(&id);
                if let Some(index) = self.index_of(id) {
                    self.devices.remove(index);
                    self.outbox.push(api::DeviceListChange {
                        kind: api::DeviceListChangeKind::Removed,
                        index,
                        device: device.expect("invariant"),
                    })
                }
            }
            DeviceChange::AppMessage(_) => { /* not relevant */ }
        }
    }

    pub fn get_device(&self, id: DeviceId) -> Option<api::ConnectedDevice> {
        self.connected.get(&id).cloned()
    }
    fn index_of(&self, id: DeviceId) -> Option<usize> {
        self.devices
            .iter()
            .enumerate()
            .find(|(_, device_id)| **device_id == id)
            .map(|(i, _)| i)
    }

    fn append(&mut self, id: DeviceId) {
        if self.index_of(id).is_none() {
            self.devices.push(id);
            self.outbox.push(crate::api::DeviceListChange {
                kind: api::DeviceListChangeKind::Added,
                index: self.devices.len() - 1,
                device: self.get_device(id).expect("invariant"),
            });
        }
    }

    pub fn set_recovery_mode(&mut self, id: DeviceId, recovery_mode: bool) {
        if let Some(connected_device) = self.connected.get_mut(&id) {
            if connected_device.recovery_mode != recovery_mode {
                connected_device.recovery_mode = recovery_mode;
                let connected_device = connected_device.clone();
                self.outbox.push(crate::api::DeviceListChange {
                    kind: api::DeviceListChangeKind::RecoveryMode,
                    index: self.index_of(id).expect("invariant"),
                    device: connected_device,
                })
            }
        }
    }
}
