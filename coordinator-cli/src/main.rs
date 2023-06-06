use anyhow::anyhow;
use clap::{Parser, Subcommand};
use coordinator_core::io::fetch_input;
use coordinator_core::wallet::Wallet;
use frostsnap_comms::DeviceReceiveBody;
use frostsnap_comms::DeviceReceiveMessage;
use frostsnap_core::CoordinatorState;
use frostsnap_core::FrostCoordinator;
use frostsnap_ext::nostr;
use std::collections::VecDeque;
use std::path::PathBuf;
use tracing::{event, Level};

pub mod wallet;

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

    let mut db = coordinator_core::db::Db::new(db_path)?;
    let changeset = db.load()?;

    // TODO ports::new(device_labels)
    let mut ports = coordinator_core::ports::Ports::default();

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
                println!("{:#?}\n", &state.key.frost_key());
                println!(
                    "32-byte key (hex): {}\n",
                    hex::encode(xonly_pk.to_xonly_bytes())
                );
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
                    let device_label = coordinator_core::device_namer::gen_name39();
                    eprintln!("Registered new device: {}", device_label);
                    ports.device_labels().insert(device_id, device_label);
                }
            }

            let keygen_devices = if ports.registered_devices().len() > n_devices {
                eprintln!("Select devices to do key generation:");
                coordinator_core::choose_devices(&ports.connected_device_labels(), n_devices)
            } else {
                ports.registered_devices().clone()
            };

            if "y"
                != coordinator_core::io::fetch_input(&format!(
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
                coordinator_core::process_outbox(
                    &mut db,
                    &mut coordinator,
                    &mut outbox,
                    &mut ports,
                )?;
                ports.send_to_devices()?;
            }
        }
        Command::Sign(sign_args) => {
            let state = changeset
                .frostsnap
                .ok_or(anyhow!("we can't sign because haven't done keygen yet!"))?;
            let coordinator = FrostCoordinator::from_stored_key(state.key);
            let mut signer =
                coordinator_core::signer::Signer::new(&mut db, &mut ports, coordinator);

            match sign_args {
                SignArgs::Message { messages } => {
                    let finished_signatures = signer.sign_message_request(
                        frostsnap_ext::sign_messages::RequestSignMessage::Plain(messages.into()),
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

                    let event =
                        nostr::UnsignedEvent::new(public_key, 1, vec![], message, time_now as i64);

                    let finished_signature = signer.sign_message_request(
                        frostsnap_ext::sign_messages::RequestSignMessage::Nostr(event.clone()),
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
