#![allow(unused)]
use crate::api::device_list as api;
use frostsnap_coordinator::{
    frostsnap_core::{AccessStructureRef, DeviceId},
    DeviceChange,
};
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
        api::DeviceListUpdate {
            changes,
            state: self.state(),
        }
    }

    /// Read-only snapshot of the current state. Unlike `take_update`, this
    /// does not drain the outbox — use it when you only need the current
    /// device set and don't want to steal pending changes from subscribers.
    pub fn state(&self) -> api::DeviceListState {
        api::DeviceListState {
            devices: self.devices(),
            state_id: self.state_counter,
        }
    }

    pub fn consume_manager_event(&mut self, change: DeviceChange) {
        match change {
            DeviceChange::Connected {
                id,
                firmware_digest,
                latest_firmware_digest,
            } => {
                use frostsnap_coordinator::FirmwareVersion;

                self.connected.insert(
                    id,
                    api::ConnectedDevice {
                        firmware: FirmwareVersion::new(firmware_digest),
                        latest_firmware: latest_firmware_digest.map(FirmwareVersion::new),
                        name: None,
                        id,
                        recovery_mode: api::RecoveryMode::Off,
                    },
                );
            }
            DeviceChange::NameChange { id, name } => {
                if let Some(index) = self.index_of(id) {
                    let connected = self.connected.get_mut(&id).expect("invariant");
                    connected.name = Some(name);
                    let device = connected.clone();
                    self.record(api::DeviceListChangeKind::Named, index, device);
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
                            let device = connected.clone();
                            self.record(api::DeviceListChangeKind::Named, index, device);
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
                    self.record(
                        api::DeviceListChangeKind::Removed,
                        index,
                        device.expect("invariant"),
                    )
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

    fn record(
        &mut self,
        kind: api::DeviceListChangeKind,
        index: usize,
        device: api::ConnectedDevice,
    ) {
        self.state_counter += 1;
        self.outbox.push(api::DeviceListChange {
            kind,
            index: index as u32,
            device,
            state_id: self.state_counter,
        });
    }

    fn append(&mut self, id: DeviceId) {
        if self.index_of(id).is_none() {
            self.devices.push(id);
            let device = self.get_device(id).expect("invariant");
            self.record(
                api::DeviceListChangeKind::Added,
                self.devices.len() - 1,
                device,
            );
        }
    }

    pub fn set_recovery_mode(&mut self, id: DeviceId, recovery_mode: api::RecoveryMode) {
        if let Some(connected_device) = self.connected.get_mut(&id) {
            // Compares the whole value, not just on/off, so a device already in recovery that
            // gains consolidations still emits. Comparing only the flag dropped that update.
            if connected_device.recovery_mode != recovery_mode {
                connected_device.recovery_mode = recovery_mode;
                let connected_device = connected_device.clone();
                let index = self.index_of(id).expect("invariant");
                self.record(
                    api::DeviceListChangeKind::RecoveryMode,
                    index,
                    connected_device,
                )
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use frostsnap_coordinator::frostsnap_comms::Sha256Digest;

    fn device(seed: u8) -> DeviceId {
        let mut bytes = [0u8; 33];
        bytes[0] = seed;
        DeviceId(bytes)
    }

    fn plug_in(list: &mut DeviceList, id: DeviceId) {
        list.consume_manager_event(DeviceChange::Connected {
            id,
            firmware_digest: Sha256Digest([0; 32]),
            latest_firmware_digest: None,
        });
        list.consume_manager_event(DeviceChange::NeedsName { id });
    }

    #[test]
    fn update_and_snapshot_of_the_same_devices_share_a_state_id() {
        let mut list = DeviceList::default();
        plug_in(&mut list, device(1));
        plug_in(&mut list, device(2));
        let added = list.take_update();
        assert_eq!(added.state.state_id, list.state().state_id);

        list.consume_manager_event(DeviceChange::Disconnected { id: device(2) });
        let removed = list.take_update();
        assert_eq!(removed.state.state_id, list.state().state_id);
        assert!(removed.state.state_id > added.state.state_id);

        let empty = list.take_update();
        assert!(empty.changes.is_empty());
        assert_eq!(empty.state.state_id, removed.state.state_id);
    }

    #[test]
    fn a_snapshot_inside_an_update_holds_exactly_the_changes_up_to_its_id() {
        let mut list = DeviceList::default();
        plug_in(&mut list, device(1));
        let snapshot = list.state();
        plug_in(&mut list, device(2));
        let update = list.take_update();

        let held: Vec<_> = update
            .changes
            .iter()
            .filter(|change| change.state_id <= snapshot.state_id)
            .map(|change| change.device.id)
            .collect();
        assert_eq!(held, vec![device(1)]);
        assert_eq!(
            update.changes.last().map(|change| change.state_id),
            Some(update.state.state_id)
        );
    }
}
