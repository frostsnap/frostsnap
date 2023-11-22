use crate::api::{self, DeviceListState};
use frostsnap_coordinator::{frostsnap_core::DeviceId, DeviceChange};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    devices: Vec<DeviceId>,
    names: HashMap<DeviceId, String>,
}

impl DeviceList {
    /// Set the internal state from the usb serial manager. This doesn't emit events.
    pub fn populate_from_port_devices(
        &mut self,
        devices_by_port: HashMap<String, Vec<DeviceId>>,
        names: HashMap<DeviceId, String>,
    ) {
        let devices = devices_by_port.into_values().flatten().collect();
        self.devices = devices;
        self.names = names;
    }

    pub fn state(&self) -> api::DeviceListState {
        let devices = self
            .devices
            .iter()
            .cloned()
            .map(|id| api::Device {
                name: self.names.get(&id).cloned(),
                id,
            })
            .collect();

        DeviceListState { devices }
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::Device> {
        self.devices.get(index).map(|id| api::Device {
            id: *id,
            name: self.names.get(id).cloned(),
        })
    }

    pub fn consume_manager_event(
        &mut self,
        changes: Vec<DeviceChange>,
    ) -> Vec<crate::api::DeviceListChange> {
        let mut output = vec![];
        for change in changes {
            match change {
                DeviceChange::Added { id: _id } => {
                    /* added events are not worth telling the user about -- we don't know if it has a name yet*/
                }
                DeviceChange::Renamed {
                    id,
                    old_name: _old_name,
                    new_name,
                } => {
                    // NOTE: ignoring old name for now
                    self.names.insert(id, new_name);
                }
                DeviceChange::NeedsName { id } => {
                    output.extend(self.append(id));
                }
                DeviceChange::Registered { id, name } => match self.index_of(id) {
                    Some(index) => {
                        if self.names.get(&id) != Some(&name) {
                            self.names.insert(id, name.clone());
                            output.push(api::DeviceListChange {
                                kind: api::DeviceListChangeKind::Named,
                                index,
                                device: api::Device {
                                    id,
                                    name: Some(name),
                                },
                            });
                        }
                    }
                    None => {
                        self.names.insert(id, name);
                        output.extend(self.append(id));
                    }
                },
                DeviceChange::Disconnected { id } => {
                    if let Some(index) = self.index_of(id) {
                        self.devices.remove(index);
                        output.push(api::DeviceListChange {
                            kind: api::DeviceListChangeKind::Removed,
                            index,
                            device: api::Device {
                                id,
                                name: self.names.get(&id).cloned(),
                            },
                        })
                    }
                }
            }
        }
        output
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
                device: api::Device {
                    id,
                    name: self.names.get(&id).cloned(),
                },
            })
        } else {
            None
        }
    }
}
