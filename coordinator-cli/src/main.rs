use anyhow::anyhow;
use bech32::ToBase32;
use bech32::Variant;
use db::Db;
use frostsnap_comms::DeviceReceiveBody;
use frostsnap_comms::DeviceReceiveMessage;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToStorageMessage;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::CoordinatorState;
use frostsnap_core::DeviceId;
use frostsnap_core::FrostCoordinator;
use ports::Ports;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::path::PathBuf;
use tracing::{event, Level};
use wallet::Wallet;

pub mod db;
mod device_namer;
pub mod io;
pub mod nostr;
pub mod ports;
pub mod serial_rw;
pub mod signer;
pub mod wallet;

use clap::{Parser, Subcommand};

use crate::io::fetch_input;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Database file (default: ~/.frostsnap)
    #[arg(short, long, value_name = "FILE")]
    db: Option<PathBuf>,
    /// Increase verbosity
    #[arg(short)]
    verbosity: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new Frostsnap key (t-of-n)
    Keygen {
        #[arg(short, long)]
        threshold: usize,
        #[arg(short, long)]
        n_devices: usize,
    },
    /// View the existing Frostsnap key
    Key,
    /// Sign a message, Bitcoin transaction, or Nostr post
    #[command(subcommand)]
    Sign(SignArgs),

    #[clap(flatten)]
    WalletCmd(wallet::Commands),
}

#[derive(Subcommand)]
enum SignArgs {
    /// Sign a plain message string
    Message { messages: String },
    /// Sign a Nostr event and broadcast
    Nostr {
        #[arg(value_name = "message")]
        message: String,
    },
    /// Sign a Bitcoin transaction
    Transaction {
        #[arg(value_name = "PSBT")]
        psbt_file: PathBuf,
    },
}

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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.verbosity {
            Level::DEBUG
        } else {
            Level::INFO
        })
        .pretty()
        .finish();
    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)?;

    let default_db_path = home::home_dir().map(|home_dir| home_dir.join(".frostsnap"));
    // use that subscriber to process traces emitted after this point

    let db_path = cli
        .db
        .or(default_db_path)
        .ok_or(anyhow!("We could not find home dir"))?;

    let mut db = db::Db::new(db_path)?;
    let changeset = db.load()?;

    // TODO ports::new(device_labels)
    let mut ports = ports::Ports::default();

    if let Some(state) = &changeset.frostsnap {
        *ports.device_labels() = state.device_labels.clone();
    }

    match cli.command {
        Command::WalletCmd(command) => {
            let frostsnap = changeset
                .frostsnap
                .ok_or(anyhow!("you haven't generated a key yet!"))?;
            let mut wallet = Wallet::new(frostsnap.key, changeset.wallet);
            command.run(
                &mut wallet,
                &mut db,
                &mut ports,
                bdk_chain::bitcoin::Network::Signet,
            )?;
        }
        Command::Key => match changeset.frostsnap {
            Some(state) => {
                let xonly_pk = state.key.frost_key().clone().into_xonly_key().public_key();
                let pk_bytes = xonly_pk.to_xonly_bytes();
                let encoded =
                    bech32::encode("npub", pk_bytes.to_base32(), Variant::Bech32).unwrap();

                println!("{:#?}\n", &state.key.frost_key());
                println!("32-byte key (hex): {}", hex::encode(pk_bytes));
                println!("Nostr: {}\n", encoded);
                println!("Known devices: {:#?}\n", &state.device_labels);
            }
            None => eprintln!("You have not generated a key yet!"),
        },
        Command::Keygen {
            threshold,
            n_devices,
        } => {
            eprintln!("Please plug in {} devices..", n_devices);

            while ports.registered_devices().len() < n_devices {
                ports.poll_devices();

                for device_id in ports.unlabelled_devices().collect::<Vec<_>>() {
                    let device_label = device_namer::gen_name39();
                    eprintln!("Registered new device: {}", device_label);
                    ports.device_labels().insert(device_id, device_label);
                }
            }

            let keygen_devices = if ports.registered_devices().len() > n_devices {
                eprintln!("Select devices to do key generation:");
                choose_devices(&ports.connected_device_labels(), n_devices)
            } else {
                ports.registered_devices().clone()
            };

            if "y"
                != io::fetch_input(&format!(
                    "Want to do keygen with these devices? [y/n]\n{}",
                    keygen_devices
                        .clone()
                        .into_iter()
                        .map(|device_id| ports
                            .device_labels()
                            .get(&device_id)
                            .expect("must exist")
                            .clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            {
                return Ok(());
            };

            let mut coordinator = frostsnap_core::FrostCoordinator::new();

            let do_keygen_message = DeviceReceiveMessage {
                target_destinations: keygen_devices.clone(),
                message_body: DeviceReceiveBody::Core(
                    coordinator.do_keygen(&keygen_devices, threshold)?,
                ),
            };
            ports.queue_in_port_outbox(vec![do_keygen_message]);

            let mut outbox = VecDeque::new();
            loop {
                let new_messages = ports.receive_messages();
                for message in new_messages {
                    match coordinator.recv_device_message(message.clone()) {
                        Ok(messages) => {
                            outbox.extend(messages);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to process message from {}: {}",
                                message.from,
                                e
                            );
                            continue;
                        }
                    };
                }
                if let CoordinatorState::FrostKey { .. } = coordinator.state() {
                    if outbox.is_empty() {
                        break;
                    }
                }
                process_outbox(&mut db, &mut coordinator, &mut outbox, &mut ports)?;
                ports.send_to_devices()?;
            }
        }
        Command::Sign(sign_args) => {
            let state = changeset
                .frostsnap
                .ok_or(anyhow!("we can't sign because haven't done keygen yet!"))?;
            let coordinator = FrostCoordinator::from_stored_key(state.key);
            let mut signer = signer::Signer::new(&mut db, &mut ports, coordinator);

            match sign_args {
                SignArgs::Message { messages } => {
                    let finished_signatures = signer.sign_message_request(
                        SignTask::Plain(messages.into()),
                        false,
                    )?;

                    println!(
                        "{}",
                        finished_signatures
                            .into_iter()
                            .map(|signature| signature.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
                SignArgs::Nostr { message } => {
                    let public_key = signer
                        .coordinator_frost_key()?
                        .frost_key()
                        .clone()
                        .into_xonly_key()
                        .public_key();
                    let time_now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("Failed to retrieve system time")
                        .as_secs();

                    let event = frostsnap_core::nostr::UnsignedEvent::new(
                        public_key,
                        1,
                        vec![],
                        message,
                        time_now as i64,
                    );

                    let finished_signature = signer.sign_message_request(
                        SignTask::Nostr(event.clone()),
                        false,
                    )?;
                    let finished_signature = finished_signature[0].clone();
                    let signed_event = event.add_signature(finished_signature);

                    println!("{}", serde_json::json!(signed_event).to_string());

                    if "y" != crate::fetch_input("Broadcast Frostr event? [y/n]") {
                        return Ok(());
                    }

                    let mut relayed = false;
                    for relay in [
                        "wss://nostr-relay.schnitzel.world",
                        "wss://relay.damus.io",
                        "wss://nostr-dev.wellorder.net",
                        "wss://nostr-relay.bitcoin.ninja",
                    ] {
                        match nostr::broadcast_event(signed_event.clone(), relay) {
                            Ok(_) => {
                                relayed = true;
                                eprintln!("Broadcasted to {relay}");
                            }
                            Err(e) => {
                                eprintln!("Failed to relay event to {relay}: {e}");
                            }
                        }
                    }
                    if relayed {
                        println!("View event: https://www.nostr.guru/e/{}", &signed_event.id);
                    }
                }
                SignArgs::Transaction { .. } => todo!(),
            }
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
