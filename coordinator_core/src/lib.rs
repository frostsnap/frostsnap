pub mod db;
pub mod device_namer;
pub mod io;
pub mod ports;
pub mod serial_rw;
pub mod signer;
pub mod wallet;

use db::Db;
use frostsnap_comms::DeviceReceiveBody;
use frostsnap_comms::DeviceReceiveMessage;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToStorageMessage;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::DeviceId;
use frostsnap_core::FrostCoordinator;
use ports::Ports;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;

pub fn process_outbox(
    db: &mut Db,
    coordinator: &mut FrostCoordinator,
    outbox: &mut VecDeque<CoordinatorSend>,
    ports: &mut Ports,
) -> anyhow::Result<()> {
    while let Some(message) = outbox.pop_front() {
        match message {
            CoordinatorSend::ToDevice(core_message) => {
                ports.queue_in_port_outbox(vec![DeviceReceiveMessage {
                    target_destinations: core_message.default_destinations(),
                    message_body: DeviceReceiveBody::Core(core_message),
                }]);
            }
            CoordinatorSend::ToUser(to_user_message) => match to_user_message {
                CoordinatorToUserMessage::Signed { .. } => {}
                CoordinatorToUserMessage::CheckKeyGen { xpub } => {
                    let ack = io::fetch_input(&format!("OK? [y/n]: {}", xpub)) == "y";
                    outbox.extend(coordinator.keygen_ack(ack)?);
                }
            },
            CoordinatorSend::ToStorage(to_storage_message) => match to_storage_message {
                CoordinatorToStorageMessage::UpdateState(key) => {
                    db.save(db::State {
                        key,
                        device_labels: ports.device_labels().clone(),
                    })?;
                }
            },
        }
    }
    Ok(())
}

pub fn choose_devices(
    device_labels: &BTreeMap<DeviceId, String>,
    n_devices: usize,
) -> BTreeSet<DeviceId> {
    let devices_vec = device_labels.iter().collect::<Vec<_>>();
    for (index, (_, device_label)) in devices_vec.iter().enumerate() {
        eprintln!("({}) - {}", index, device_label);
    }

    let mut chosen_signers: BTreeSet<DeviceId> = BTreeSet::new();
    while chosen_signers.len() < n_devices {
        let choice = io::fetch_input("\nEnter a device index (n): ").parse::<usize>();
        match choice {
            Ok(n) => match devices_vec.get(n) {
                Some((device_id, _)) => {
                    if !chosen_signers.contains(device_id) {
                        chosen_signers.insert(**device_id);
                    } else {
                        eprintln!("Already chose this device!")
                    }
                }
                None => eprintln!("no such device ({}", n),
            },
            Err(_) => {
                eprintln!("Choose a number 0..{}", devices_vec.len() - 1);
            }
        }
    }
    chosen_signers
}
