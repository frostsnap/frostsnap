use crate::api::{self, DeviceListState};
use frostsnap_coordinator::{frostsnap_core::DeviceId, DeviceChange};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    devices: Vec<DeviceId>,
    connected: HashMap<DeviceId, api::ConnectedDevice>,
    state_counter: usize,
}

impl DeviceList {
    pub fn state(&self) -> api::DeviceListState {
        let devices = self
            .devices
            .iter()
            .cloned()
            .map(|id| self.connected.get(&id).cloned().expect("invariant"))
            .collect();

        DeviceListState {
            devices,
            state_id: self.state_counter,
        }
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::ConnectedDevice> {
        self.devices
            .get(index)
            .map(|id| self.connected.get(id).cloned().expect("invariant"))
    }

    pub fn consume_manager_event(
        &mut self,
        changes: Vec<api::DeviceChange>,
    ) -> Vec<crate::api::DeviceListChange> {
        let mut output = vec![];
        for change in changes {
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
                    });

                    connected.firmware_digest = firmware_digest.to_string();
                    connected.latest_digest =
                        latest_firmware_digest.map(|digest| digest.to_string());
                }
                DeviceChange::NameChange { id, name } => {
                    if let Some(index) = self.index_of(id) {
                        let connected = self.connected.get_mut(&id).expect("invariant");
                        connected.name = Some(name);
                        output.push(api::DeviceListChange {
                            kind: api::DeviceListChangeKind::Named,
                            index,
                            device: connected.clone(),
                        });
                    }
                }
                DeviceChange::NeedsName { id } => {
                    output.extend(self.append(id));
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
                                output.push(api::DeviceListChange {
                                    kind: api::DeviceListChangeKind::Named,
                                    index,
                                    device: connected.clone(),
                                });
                            }
                        }
                        None => {
                            connected.name = Some(name);
                            output.extend(self.append(id));
                        }
                    }
                }
                DeviceChange::Disconnected { id } => {
                    let device = self.connected.remove(&id);
                    if let Some(index) = self.index_of(id) {
                        self.devices.remove(index);
                        output.push(api::DeviceListChange {
                            kind: api::DeviceListChangeKind::Removed,
                            index,
                            device: device.expect("invariant"),
                        })
                    }
                }
                DeviceChange::AppMessage(_) => { /* not relevant */ }
            }
        }

        if !output.is_empty() {
            self.state_counter += 1;
        }
        output
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

    #[must_use]
    fn append(&mut self, id: DeviceId) -> Option<crate::api::DeviceListChange> {
        if self.index_of(id).is_none() {
            self.devices.push(id);
            Some(crate::api::DeviceListChange {
                kind: api::DeviceListChangeKind::Added,
                index: self.devices.len() - 1,
                device: self.get_device(id)?,
            })
        } else {
            None
        }
    }
}
