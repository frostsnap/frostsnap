use anyhow::anyhow;
use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::message::DeviceToCoordinatorBody;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{event, Level};

pub mod db;
mod device_namer;
pub mod io;
pub mod ports;
pub mod serial_rw;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(short, long, value_name = "FILE")]
    db: Option<PathBuf>,
    #[arg(short)]
    verbosity: bool,
}

#[derive(Subcommand)]
enum Command {
    Keygen {
        #[arg(short, long)]
        threshold: usize,
        #[arg(short, long)]
        n_devices: usize,
    },
    Key,
    #[command(subcommand)]
    Sign(SignArgs),
}

#[derive(Subcommand)]
enum SignArgs {
    Message {
        #[arg(value_name = "message")]
        message: String,
    },
    Nostr {
        #[arg(value_name = "message")]
        message: String,
    },
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

    let db = db::Db::new(db_path);
    let state = db.load()?;

    let mut ports = ports::Ports::default();

    if let Some(state) = &state {
        *ports.device_labels() = state.device_labels.clone();
    }

    match cli.command {
        Command::Key => match state {
            Some(state) => {
                println!("{:?}", state.key);
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

            if "y"
                != io::fetch_input(&format!(
                    "Want to do keygen with these devices? [y/n]\n{}",
                    ports
                        .registered_devices()
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

            let do_keygen_message = DeviceReceiveSerial::Core(
                coordinator.do_keygen(&ports.registered_devices(), threshold)?,
            );
            ports.send_to_all_devices(&do_keygen_message)?;

            let mut finished_keygen = false;
            while !finished_keygen {
                let new_messages = ports.receive_messages();
                for message in new_messages {
                    match coordinator.recv_device_message(message.clone()) {
                        Ok(responses) => {
                            for response in responses {
                                match response {
                                    CoordinatorSend::ToDevice(core_message) => {
                                        ports.send_to_all_devices(&DeviceReceiveSerial::Core(
                                            core_message,
                                        ))?;
                                    }
                                    CoordinatorSend::ToUser(to_user_message) => {
                                        match to_user_message {
                                            frostsnap_core::message::CoordinatorToUserMessage::Signed { .. } => {}
                                            frostsnap_core::message::CoordinatorToUserMessage::CheckKeyGen {
                                                xpub
                                            } => {
                                                let ack = io::fetch_input(&format!("OK? [y/n]: {}", xpub)) == "y";
                                                if let Some(key) = coordinator.keygen_ack(ack).unwrap() {
                                                    db.save(db::State {
                                                        key,
                                                        device_labels: ports.device_labels().clone(),
                                                    })?;
                                                }
                                                finished_keygen = true;

                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to receive message from {}: {}",
                                message.from,
                                e
                            );
                            continue;
                        }
                    };
                }
            }
        }
        Command::Sign(sign_args) => {
            let state = state.ok_or(anyhow!("we can't sign because haven't done keygen yet!"))?;
            let key = state.key;
            let threshold = key.threshold();

            let key_signers: HashMap<_, _> = key
                .devices()
                .map(|device_id| {
                    (
                        device_id,
                        state
                            .device_labels
                            .get(&device_id)
                            .expect("device in key must be known to coordinator")
                            .to_string(),
                    )
                })
                .collect();

            let chosen_signers = if key_signers.len() != threshold {
                choose_signers(&key_signers, threshold)
            } else {
                key_signers.keys().cloned().collect()
            };

            let mut still_need_to_sign = chosen_signers.clone();
            let mut coordinator = frostsnap_core::FrostCoordinator::from_stored_key(key);

            match sign_args {
                SignArgs::Message { message } => {
                    // TODO remove unwrap --> anyhow
                    let sign_request = coordinator.start_sign(message, chosen_signers).unwrap();

                    eprintln!(
                        "Plug signers:\n{}",
                        still_need_to_sign
                            .iter()
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    loop {
                        let (newly_registered, new_messages) = ports.poll_devices();
                        for device in newly_registered.intersection(&still_need_to_sign) {
                            event!(Level::INFO, "asking {} to sign", device);
                            ports.send_to_single_device(
                                &DeviceReceiveSerial::Core(sign_request.clone()),
                                device,
                            )?;
                        }

                        for message in new_messages {
                            match coordinator.recv_device_message(message.clone()) {
                                Ok(responses) => {
                                    for response in responses {
                                        match response {
                                            CoordinatorSend::ToDevice(core_message) => {
                                                // TODO: Send response back to particular device?
                                                ports.send_to_all_devices(
                                                    &DeviceReceiveSerial::Core(core_message),
                                                )?;
                                            }
                                            CoordinatorSend::ToUser(user_message) => {
                                                if let CoordinatorToUserMessage::Signed {
                                                    signature,
                                                } = user_message
                                                {
                                                    event!(Level::INFO, "signing complete 🎉");
                                                    println!("{}", signature);
                                                    return Ok(());
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    event!(
                                        Level::ERROR,
                                        error = e.to_string(),
                                        from = message.from.to_string(),
                                        "got invalid message"
                                    );
                                }
                            }

                            if let DeviceToCoordindatorMessage {
                                from,
                                body: DeviceToCoordinatorBody::SignatureShare { .. },
                            } = &message
                            {
                                still_need_to_sign.remove(from);
                            }
                        }
                    }
                }
                SignArgs::Nostr { .. } => todo!(),
                SignArgs::Transaction { .. } => todo!(),
            }
        }
    }
    Ok(())
}

fn choose_signers(
    device_labels: &HashMap<DeviceId, String>,
    threshold: usize,
) -> BTreeSet<DeviceId> {
    eprintln!("Choose {} devices to sign:", threshold);
    let devices_vec = device_labels.iter().collect::<Vec<_>>();
    for (index, (_, device_label)) in devices_vec.iter().enumerate() {
        eprintln!("({}) - {}", index, device_label);
    }

    let mut chosen_signers: BTreeSet<DeviceId> = BTreeSet::new();
    while chosen_signers.len() < threshold {
        let choice = io::fetch_input("\nEnter a signer index (n): ").parse::<usize>();
        match choice {
            Ok(n) => match devices_vec.get(n) {
                Some((device_id, _)) => {
                    if !chosen_signers.contains(device_id) {
                        chosen_signers.insert(**device_id);
                    } else {
                        eprintln!("Already chose this signer!")
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
