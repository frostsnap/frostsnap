use crate::api::{self, DeviceListState};
use frostsnap_coordinator::{frostsnap_core::DeviceId, DeviceChange};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    devices: Vec<DeviceId>,
    metadata: HashMap<DeviceId, DeviceData>,
    state_counter: usize,
}

#[derive(Debug, Clone, Default)]
struct DeviceData {
    firmware_digest: String,
    latest_digest: String,
    name: Option<String>,
}

impl DeviceData {
    pub fn into_device(self, id: DeviceId) -> api::Device {
        api::Device {
            id,
            latest_digest: self.latest_digest,
            firmware_digest: self.firmware_digest.clone(),
            name: self.name.clone(),
        }
    }
}

impl DeviceList {
    pub fn state(&self) -> api::DeviceListState {
        let devices = self
            .devices
            .iter()
            .cloned()
            .map(|id| {
                self.metadata
                    .get(&id)
                    .cloned()
                    .unwrap_or_default()
                    .into_device(id)
            })
            .collect();

        DeviceListState {
            devices,
            state_id: self.state_counter,
        }
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::Device> {
        self.devices.get(index).map(|id| {
            self.metadata
                .get(id)
                .cloned()
                .unwrap_or_default()
                .into_device(*id)
        })
    }

    pub fn consume_manager_event(
        &mut self,
        changes: Vec<DeviceChange>,
    ) -> Vec<crate::api::DeviceListChange> {
        let mut output = vec![];
        for change in changes {
            match change {
                DeviceChange::Connected {
                    id,
                    firmware_digest,
                    latest_firmware_digest,
                } => {
                    self.metadata
                        .entry(id)
                        .and_modify(|metadata| {
                            metadata.firmware_digest = firmware_digest.to_string();
                            metadata.latest_digest = latest_firmware_digest.to_string();
                        })
                        .or_insert(DeviceData {
                            firmware_digest: firmware_digest.to_string(),
                            latest_digest: latest_firmware_digest.to_string(),
                            ..Default::default()
                        });
                }
                DeviceChange::Renamed {
                    id,
                    old_name: _old_name,
                    new_name,
                } => {
                    if let Some(index) = self.index_of(id) {
                        // NOTE: ignoring old name for now
                        let device_data = self
                            .metadata
                            .entry(id)
                            .and_modify(|device| device.name = Some(new_name.clone()))
                            .or_insert(DeviceData {
                                name: Some(new_name),
                                ..Default::default()
                            });
                        output.push(api::DeviceListChange {
                            kind: api::DeviceListChangeKind::Named,
                            index,
                            device: device_data.clone().into_device(id),
                        });
                    }
                }
                DeviceChange::NeedsName { id } => {
                    output.extend(self.append(id));
                }
                DeviceChange::Registered { id, name } => {
                    let index = self.index_of(id);
                    let metadata = self.metadata.entry(id).or_default();

                    match index {
                        Some(index) => {
                            if metadata.name.is_some() {
                                assert_eq!(
                                    metadata.name,
                                    Some(name.clone()),
                                    "we should have got a renamed event if they were different"
                                );
                            } else {
                                metadata.name = Some(name);
                                output.push(api::DeviceListChange {
                                    kind: api::DeviceListChangeKind::Named,
                                    index,
                                    device: metadata.clone().into_device(id),
                                });
                            }
                        }
                        None => {
                            metadata.name = Some(name);
                            output.extend(self.append(id));
                        }
                    }
                }
                DeviceChange::Disconnected { id } => {
                    if let Some(index) = self.index_of(id) {
                        self.devices.remove(index);
                        output.push(api::DeviceListChange {
                            kind: api::DeviceListChangeKind::Removed,
                            index,
                            device: self
                                .metadata
                                .get(&id)
                                .cloned()
                                .unwrap_or_default()
                                .into_device(id),
                        })
                    }
                }
                DeviceChange::AppMessage(_) => { /* not relevant */ }
                DeviceChange::NewUnknownDevice { .. } => {
                    /* TODO: a new device should prompt the user to sync or something */
                }
            }
        }

        if !output.is_empty() {
            self.state_counter += 1;
        }
        output
    }

    pub fn get_device(&self, id: DeviceId) -> api::Device {
        self.metadata
            .get(&id)
            .cloned()
            .unwrap_or_default()
            .into_device(id)
    }

    pub fn init_names(&mut self, names: HashMap<DeviceId, String>) {
        self.metadata = names
            .into_iter()
            .map(|(id, name)| {
                (
                    id,
                    DeviceData {
                        name: Some(name),
                        ..Default::default()
                    },
                )
            })
            .collect();
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
                device: self.get_device(id),
            })
        } else {
            None
        }
    }
}
