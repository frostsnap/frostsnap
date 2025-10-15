#![allow(unused)]
use crate::api::device_list as api;
use frostsnap_coordinator::{frostsnap_core::DeviceId, DeviceChange};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    devices: Vec<DeviceId>,
    connected: HashMap<DeviceId, api::ConnectedDevice>,
    state_counter: u32,
    outbox: Vec<api::DeviceListChange>,
}

impl DeviceList {
    pub fn update_ready(&self) -> bool {
        !self.outbox.is_empty()
    }

    // Order devices by connection order, with any new devices appended
    pub fn sort_as_connected(
        &self,
        devices: HashSet<DeviceId>,
    ) -> impl Iterator<Item = DeviceId> + '_ {
        let ordered_devices: Vec<_> = self
            .devices
            .iter()
            .filter(|id| devices.contains(id))
            .copied()
            .collect();

        let remaining_devices: Vec<_> = devices
            .into_iter()
            .filter(|id| !self.devices.contains(id))
            .collect();

        ordered_devices.into_iter().chain(remaining_devices)
    }

    pub fn devices(&self) -> Vec<api::ConnectedDevice> {
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
            state: api::DeviceListState {
                devices: self.devices(),
                state_id: self.state_counter,
            },
        };

        if !update.changes.is_empty() {
            self.state_counter += 1;
        }

        update
    }

    pub fn consume_manager_event(&mut self, change: DeviceChange) {
        match change {
            DeviceChange::Connected {
                id,
                firmware_digest,
                latest_firmware_digest,
            } => {
                let connected = self.connected.entry(id).or_insert(api::ConnectedDevice {
                    firmware_digest: Default::default(),
                    latest_digest: Default::default(),
                    name: None,
                    id,
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
                        index: index as u32,
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
                                index: index as u32,
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
                        index: index as u32,
                        device: device.expect("invariant"),
                    })
                }
            }
            DeviceChange::AppMessage(_) => { /* not relevant */ }
            DeviceChange::GenuineDevice { .. } => { /* not displayed in app yet */ }
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
            self.outbox.push(api::DeviceListChange {
                kind: api::DeviceListChangeKind::Added,
                index: (self.devices.len() - 1) as u32,
                device: self.get_device(id).expect("invariant"),
            });
        }
    }

    pub fn set_recovery_mode(&mut self, id: DeviceId, recovery_mode: bool) {
        if let Some(connected_device) = self.connected.get_mut(&id) {
            if connected_device.recovery_mode != recovery_mode {
                connected_device.recovery_mode = recovery_mode;
                let connected_device = connected_device.clone();
                self.outbox.push(api::DeviceListChange {
                    kind: api::DeviceListChangeKind::RecoveryMode,
                    index: self.index_of(id).expect("invariant") as u32,
                    device: connected_device,
                })
            }
        }
    }
}
