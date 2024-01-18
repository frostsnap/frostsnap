use anyhow::anyhow;
use bech32::ToBase32;
use bech32::Variant;
use db::Db;
use frostsnap_comms::CoordinatorSendBody;
use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_comms::Destination;
use frostsnap_coordinator::DesktopSerial;
use frostsnap_coordinator::DeviceChange;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToStorageMessage;
use frostsnap_core::message::CoordinatorToUserKeyGenMessage;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::message::CoordinatorToUserSigningMessage;
use frostsnap_core::CoordinatorState;
use frostsnap_core::DeviceId;
use frostsnap_core::FrostCoordinator;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::path::PathBuf;
use tracing::{event, Level};
use wallet::Wallet;

pub mod db;
pub mod nostr;
pub mod signer;
pub mod wallet;

use clap::{Parser, Subcommand};

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
    /// Set up devices
    Setup,
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

fn process_outbox(
    db: &mut Db,
    _coordinator: &mut FrostCoordinator,
    outbox: &mut VecDeque<CoordinatorSend>,
    ports: &mut frostsnap_coordinator::UsbSerialManager,
) -> anyhow::Result<()> {
    while let Some(message) = outbox.pop_front() {
        match message {
            CoordinatorSend::ToDevice(core_message) => {
                ports.queue_in_port_outbox(CoordinatorSendMessage {
                    target_destinations: Destination::Particular(
                        core_message.default_destinations(),
                    ),
                    message_body: CoordinatorSendBody::Core(core_message),
                });
            }
            CoordinatorSend::ToUser(to_user_message) => match to_user_message {
                CoordinatorToUserMessage::Signing(message) => match message {
                    CoordinatorToUserSigningMessage::GotShare { from } => {
                        eprintln!(
                            "Got a share from {}",
                            ports
                                .device_labels()
                                .get(&from)
                                .map(String::as_str)
                                .unwrap_or("<unknown>")
                        );
                    }
                    CoordinatorToUserSigningMessage::Signed { .. } => {
                        eprintln!("Signing completed 🎉");
                    }
                },
                CoordinatorToUserMessage::KeyGen(message) => match message {
                    CoordinatorToUserKeyGenMessage::ReceivedShares { from: id } => {
                        eprintln!(
                            "✓ received share from {}",
                            ports
                                .connected_device_labels()
                                .get(&id)
                                .expect("device must be named")
                        );
                    }
                    CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                        eprintln!("Check all devices show {}", hex::encode(session_hash));
                    }
                    CoordinatorToUserKeyGenMessage::KeyGenAck { from: id } => {
                        eprintln!(
                            "✓ Got confirmation from {}",
                            ports
                                .connected_device_labels()
                                .get(&id)
                                .expect("device must be named")
                        );
                    }
                    CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
                        eprintln!("🎉 key was successfully generated {}", key_id);
                    }
                },
            },
            CoordinatorSend::ToStorage(to_storage_message) => match to_storage_message {
                CoordinatorToStorageMessage::StoreSigningState(_) => {
                    /* we don't persist this on the cli */
                }
                CoordinatorToStorageMessage::UpdateFrostKey(key) => {
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
        .without_time()
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

    let mut ports = frostsnap_coordinator::UsbSerialManager::new(Box::new(DesktopSerial));

    if let Some(state) = &changeset.frostsnap {
        *ports.device_labels_mut() = state.device_labels.clone();
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
        Command::Setup => {
            eprintln!("Plug in devices to set them up.");
            let mut waiting_for_name_ack = BTreeSet::default();

            loop {
                let port_changes = ports.poll_ports();
                for device_change in port_changes.device_changes {
                    match device_change {
                        DeviceChange::Connected { .. } | DeviceChange::Disconnected { .. } => { /* ignore */
                        }
                        DeviceChange::NewUnknownDevice { id, name } => {
                            eprintln!("New device added '{name}' with id {id}")
                        }
                        DeviceChange::Renamed {
                            id,
                            old_name,
                            new_name,
                        } => {
                            eprintln!(
                                "⚠ device {id} renamed to {new_name}. It's old name was {old_name}"
                            );
                        }
                        DeviceChange::NeedsName { id } => {
                            eprintln!("🤖 new device connected. Give it a name:");
                            let mut line = String::new();
                            std::io::stdin().read_line(&mut line)?;
                            line.pop();
                            ports.finish_naming(id, &line);
                            eprintln!("Confirm name {line} on device");
                            waiting_for_name_ack.insert(id);
                        }
                        DeviceChange::Registered { id, name } => {
                            if waiting_for_name_ack.remove(&id) {
                                eprintln!("Device {name} is ready to use!");
                            }
                        }
                    }
                }
            }
        }
        Command::Keygen {
            threshold,
            n_devices,
        } => {
            eprintln!("Please plug in {} devices..", n_devices);

            while ports.registered_devices().len() < n_devices {
                ports.poll_ports();
            }

            let keygen_devices = if ports.registered_devices().len() > n_devices {
                eprintln!("Select devices to do key generation:");
                choose_devices(&ports.connected_device_labels(), n_devices)
            } else {
                ports.registered_devices().clone()
            };

            if "y"
                != fetch_input(&format!(
                    "🤖 {}\n\nWant to do keygen with these devices? [y/n]",
                    keygen_devices
                        .clone()
                        .into_iter()
                        .map(|device_id| ports
                            .device_labels()
                            .get(&device_id)
                            .expect("must exist")
                            .clone())
                        .collect::<Vec<_>>()
                        .join("\n🤖 "),
                ))
            {
                return Ok(());
            };
            eprintln!("\nStarting FROST key generation...");

            let mut coordinator = frostsnap_core::FrostCoordinator::new();

            let do_keygen_message = CoordinatorSendMessage {
                target_destinations: Destination::Particular(keygen_devices.clone()),
                message_body: CoordinatorSendBody::Core(
                    coordinator.do_keygen(&keygen_devices, threshold)?,
                ),
            };
            ports.queue_in_port_outbox(do_keygen_message);

            let mut outbox = VecDeque::new();
            loop {
                let port_changes = ports.poll_ports();

                for (from, message) in port_changes.new_messages {
                    event!(
                        Level::DEBUG,
                        from = from.to_string(),
                        kind = message.kind(),
                        "received message during keygen"
                    );

                    match coordinator.recv_device_message(from, message) {
                        Ok(messages) => {
                            outbox.extend(messages);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                from = from.to_string(),
                                "Failed to process message {}",
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
                        frostsnap_core::message::SignTask::Plain(messages.into()),
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
                    let public_key = signer.frost_key()?.clone().into_xonly_key().public_key();
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
                        frostsnap_core::message::SignTask::Nostr(Box::new(event.clone())),
                    )?;
                    let finished_signature = finished_signature[0].clone();
                    let signed_event = event.add_signature(finished_signature);

                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!(signed_event))?
                    );

                    if "y" != crate::fetch_input("\n⚡❄ Broadcast Frostr event? [y/n]") {
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
                                eprintln!("📡 Broadcasted to {relay}");
                            }
                            Err(e) => {
                                eprintln!("Failed to relay event to {relay}: {e}");
                            }
                        }
                    }
                    if relayed {
                        println!(
                            "\n🔍 View event: https://www.nostr.guru/e/{}",
                            &signed_event.id
                        );
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
        let choice = fetch_input("\nEnter a device index (n): ").parse::<usize>();
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

fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("can not read user input");
    let cleaned_input = input.trim().to_string();
    cleaned_input
}

pub fn fetch_input(prompt: &str) -> String {
    println!("{}", prompt);
    read_string()
}
