//! Functions for handling communication rounds based on messages, expected peers, acks, and so on
//! Proobably needs rewriting and refactoring into something more robust, though this may depend
//! on the method of DeviceIO used

use std::collections::HashMap;

use log::debug;
use log::error;
use log::info;
use rand::rngs::ThreadRng;
use schnorr_fun::frost;
use schnorr_fun::frost::FrostKey;
use schnorr_fun::fun::marker::EvenY;
use schnorr_fun::fun::marker::Normal;
use schnorr_fun::fun::Point;
use schnorr_fun::fun::Scalar;
use schnorr_fun::fun::XOnlyKeyPair;
use schnorr_fun::nonce::GlobalRng;
use schnorr_fun::nonce::Synthetic;
use schnorr_fun::Schnorr;

use crate::io::*;
use crate::message::SetupMessage;
use crate::message::SetupMessage::ShareConfig;
use crate::message::*;
use crate::OUR_INDEX;
use sha2::Sha256;

/// Receive all the [`FrostMessage`]s from peers on multiple IO ports
///
/// Filters out our own messages (if someone sent it to us)
/// Filters out unexpected responses (if expected_parties is set)
///
/// # Returns
///
/// A vector of tuples containing (origin_uart_index, Frostmessage)
pub fn receive_peer_messages_from_io(
    io_ports: &mut [impl DeviceIO],
    expected_parties: Option<HashMap<Point<EvenY>, usize>>,
    our_pubkey: Point<EvenY>,
) -> Vec<(usize, FrostMessage)> {
    // Get every message with an indication of which uart they originated from
    let mut received_messages = vec![];
    for (io_idx, io) in io_ports.iter_mut().enumerate() {
        for message in io.read_messages().into_iter() {
            received_messages.push((io_idx, message));
        }
    }

    // TODO: Filter out invalid messages
    // Filter out unexpected messages
    received_messages.retain(|(_, message)| {
        if let Some(expected_parties) = &expected_parties {
            if message.sender == our_pubkey {
                error!("Received our own message from serial...");
                return false;
            }

            if !expected_parties
                .into_iter()
                .map(|(pk, _)| *pk)
                .collect::<Vec<_>>()
                .contains(&message.sender)
            {
                error!("Already have a message from this participant..");
                return false;
            }
        }
        true
    });
    received_messages
}

/// Send a message over IO, read all messages, and forward if appropriate.
///
/// TODO: The schnorr signatures attached to messages should be verified against their public key.
pub fn fetch_send_forward(
    io_ports: &mut [impl DeviceIO],
    our_messages: Vec<FrostMessage>,
    expected_parties: Option<HashMap<Point<EvenY>, usize>>,
) -> Vec<FrostMessage> {
    // Receive messages from peers on uart
    let received_messages =
        receive_peer_messages_from_io(io_ports, expected_parties, our_messages[0].sender);

    // Send our forwards and our messages
    for (io_idx, io) in io_ports.iter_mut().enumerate() {
        // Send messages to UARTs which they did not originate
        let mut messages_to_forward: Vec<_> = received_messages
            .clone()
            .into_iter()
            .filter_map(|(origin_idx, message)| {
                if origin_idx != io_idx {
                    Some(message)
                } else {
                    None
                }
            })
            .collect();

        // // Send our own messages and the forwards
        // messages_to_forward.extend(our_messages.clone());
        // messages_to_forward.reverse();
        // println!("Sending out all these messages:");
        // dbg!(&messages_to_forward);

        messages_to_forward = if messages_to_forward.len() == 0 {
            info!(
                "{}",
                &format!("Sharing our messages to io port {}..", io_idx)
            );
            our_messages.clone()
        } else {
            info!("{}", &format!("Forwarding messages to io port {}:", io_idx));
            messages_to_forward
        };
        let message_printout = messages_to_forward
            .iter()
            .map(|m| &m.message)
            .collect::<Vec<_>>();

        info!("{:?}", message_printout);

        // messages_to_forward = if messages_to_forward.len() == 0 {
        //     info!("{}", &format!("Sharing our messages to uart {}..", io_idx));
        //     info!("{:?}", our_messages.clone());
        //     our_messages.clone()
        // } else {
        //     info!("{}", &format!("Forwarding messages to uart {}:", io_idx));
        //     info!("{:?}", messages_to_forward);
        //     messages_to_forward
        // };

        io.write_messages(messages_to_forward);
    }

    received_messages
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

/// Share and receive a [`FrostMessage`] with a number of participants.
///
/// The group of expected responders can be defined via `expected_parties` argument, or using a generic `receive_n` (priority)
/// if their public keys are not yet known (e.g. during agreeing on FrostSetup).
///
/// Parties continually send out their Message until they see every other party is ready to continue.
/// Once you receieve all your messages, the message you send out will include a continue ack.
pub fn do_communication_round<ExpectedMessageType>(
    io_ports: &mut [impl DeviceIO],
    our_message: FrostMessage,
    our_key: &XOnlyKeyPair,
    expected_parties: Option<HashMap<Point<EvenY>, usize>>,
    receive_n: Option<usize>,
) -> HashMap<Point<EvenY>, FrostMessage> {
    let mut sending_message = our_message;
    let mut round_messages: HashMap<Point<EvenY>, FrostMessage> = HashMap::new();
    round_messages.insert(our_key.public_key(), sending_message.clone());

    // How many messages to search for
    let search_limit = if let Some(parties) = expected_parties.clone() {
        parties.len()
    } else if let Some(n) = receive_n {
        n
    } else {
        0
    };

    let mut ready_counter = 0;
    loop {
        // If we see enough messages then we want to send an ack that we are read to move on
        if round_messages.len() == search_limit {
            info!("We are ready to continue.. Broadcasting ack..");
            sending_message = sending_message.clone().ready_to_continue();
            round_messages.insert(sending_message.sender, sending_message.clone());
        }

        // Fetch peer messages, send our message and make forwards
        let new_messages = fetch_send_forward(
            io_ports,
            vec![sending_message.clone()],
            expected_parties.clone(),
        );

        // Have we seen a message from this peer? Handle their message appropriately
        for mut new_message in new_messages.into_iter() {
            if round_messages.contains_key(&new_message.sender) {
                error!("Already received a message from {}", new_message.sender);
            }

            // If we have already seen a message from them, check whether they have sent something new:
            if let Some(existing_message) = round_messages.get(&new_message.sender) {
                // If this is a different message
                if existing_message.signature != new_message.signature {
                    // Update their continue ack for them..
                    if !existing_message.continue_ack && sending_message.continue_ack {
                        error!("New message does not have an ack -- assuming they were ready since we are.");
                        let ready_old_message = existing_message.clone().ready_to_continue();
                        new_message = ready_old_message;
                    }
                }
            } else {
                // Check we do not get too many messages!
                if round_messages.len() >= search_limit {
                    error!("Ignoring extra messages from other participants, this round is full");
                    continue;
                }
            }

            // If the enum variants are the same (?!?!)
            if std::mem::discriminant(&new_message.message.clone())
                == std::mem::discriminant(&sending_message.message)
            {
                round_messages.insert(new_message.sender, new_message);
            } else {
                error!(
                    "Unexpected message type: {:?} -- not storing.",
                    new_message.message
                );
            }
        }

        // Get latest number of Acks
        let parties_acked_to_continue = round_messages
            .iter()
            .filter_map(|(_, msg)| if msg.continue_ack { Some(true) } else { None })
            .collect::<Vec<_>>()
            .len();

        info!(
            "Messages from {}/{} parties -- {}/{} are ready to continue.",
            round_messages.len(),
            search_limit,
            parties_acked_to_continue,
            search_limit
        );

        // If enough people have acked, we are done
        if parties_acked_to_continue == search_limit {
            ready_counter += 1;
            info!("Finished communication round since we have enough acks..");
            if ready_counter > 2 {
                break;
            }
            // } else if round_messages.len() == search_limit && acks_left <= 0 {
            //     info!("Finished communication round early since we have enough messages..");
            //     break;
        }
    }
    round_messages
}

/// Complete FROST keygen by carrying out messaging sharing rounds of:
/// [`KeygenMessage::ShareConfig`], [`KeygenMessage::Polynomial`], [`KeygenMessage::SecretShares`]
pub fn process_keygen(io_ports: &mut [impl DeviceIO]) -> (Scalar, FrostKey<Normal>) {
    let nonce_gen = Synthetic::<Sha256, GlobalRng<ThreadRng>>::default();
    let schnorr = Schnorr::<Sha256, _>::new(nonce_gen.clone());
    let frost = frost::Frost::new(schnorr.clone());
    let mut rng = rand::thread_rng();

    // Let's try to a FROST keygen over serial using our messages.

    // First let's choose our setup
    let setup = FrostSetup {
        n_parties: 2,
        threshold: 2,
        our_index: OUR_INDEX,
    };

    // Secrets
    let device_key = schnorr.new_keypair(Scalar::random(&mut rand::thread_rng()));
    let scalar_poly = frost::generate_scalar_poly(setup.threshold, &mut rng);
    let public_poly = frost::to_point_poly(&scalar_poly);

    // KeygenMessage:::ShareConfig
    let participants = {
        let setup_message = FrostMessage::new(
            &schnorr,
            &device_key,
            MessageItem::SetupMessage(ShareConfig(setup)),
        );
        info!("Finding other participants with matching Frost settings..");
        // debug!(&setup_message);

        let setup_messages = loop {
            // Let's scan for n_parties number of Messages
            let received_messages = do_communication_round::<SetupMessage>(
                io_ports,
                setup_message.clone(),
                &device_key,
                None,
                Some(setup.n_parties),
            );
            // Then let's check if we can find SetupMessages, in the quantity required
            let setup_messages: HashMap<Point<EvenY>, FrostSetup> = received_messages
                .iter()
                .filter_map(|(sender, message)| {
                    if let MessageItem::SetupMessage(SetupMessage::ShareConfig(received_setup)) =
                        message.message
                    {
                        // If we agree on the config
                        if (setup.n_parties == received_setup.n_parties)
                            & (setup.threshold == received_setup.threshold)
                        {
                            Some((*sender, received_setup))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            // Check we have no duplicate indexes in our setups
            if setup_messages
                .iter()
                .map(|(_, setup)| (setup.our_index, 0))
                .collect::<HashMap<_, _>>()
                .len()
                == setup_messages.len()
            {
                if setup_messages.len() == setup.n_parties {
                    break setup_messages;
                }
            } else {
                error!("People are broadcasting duplicate partipant indexes..");
            }
        };
        let participants = setup_messages
            .into_iter()
            .map(|(pk, setup)| (pk, setup.our_index))
            .collect::<HashMap<_, _>>();
        info!(
            "Initiated FROST setup with participants: {:?}",
            &participants
        );
        info!(" \n\n");
        participants
    };

    // KeygenMessage:::Polynomial
    let collected_polys = {
        let poly_message = FrostMessage::new(
            &schnorr,
            &device_key,
            MessageItem::KeygenPolyMessage(KeygenPolyMessage::Polynomial(public_poly.clone())),
        );
        info!("Sharing polynomials...");
        // debug!(&poly_message);

        let received_messages = do_communication_round::<KeygenPolyMessage>(
            io_ports,
            poly_message,
            &device_key,
            Some(participants.clone()),
            None,
        );
        debug!("{:?}", &received_messages);
        let mut polynomials: Vec<(_, _)> = received_messages
            .into_iter()
            .filter_map(|(sender, message)| {
                if let MessageItem::KeygenPolyMessage(KeygenPolyMessage::Polynomial(polynomial)) =
                    message.message
                {
                    Some((
                        participants.get(&sender).expect("sender exists").clone(),
                        polynomial,
                    ))
                } else {
                    None
                }
            })
            .collect();

        info!("Received polynomials!");
        info!(" \n\n");

        polynomials.sort_by(|a, b| a.0.cmp(&b.0));
        let collected_polys: Vec<_> = polynomials.into_iter().map(|(_, poly)| poly).collect();
        collected_polys
    };

    debug!("{} {}", collected_polys.len(), setup.n_parties);

    info!("Calculating keygen...");
    // debug!(&collected_polys);
    let keygen = frost
        .new_keygen(collected_polys)
        .expect("something wrong with what was provided by other parties");

    // KeygenMessage:::SecretShares
    let (collected_secret_shares, collected_pops) = {
        info!("Creating shares...");
        let (my_shares, my_pop) = frost.create_shares(&keygen, scalar_poly);
        // For now, let's just publically broadcast all the secret shares at once
        //
        // TODO: these secret shares should be ECDH encrypted to the recipients public key, and appropriately decrypted for use.
        let shares_message = FrostMessage::new(
            &schnorr,
            &device_key,
            MessageItem::KeygenSharesMessage(KeygenSharesMessage::SecretShares(my_shares, my_pop)),
        );
        info!("Sharing secret shares and proof of possession..");
        // debug!(&shares_message);

        let received_messages = do_communication_round::<KeygenSharesMessage>(
            io_ports,
            shares_message,
            &device_key,
            Some(participants.clone()),
            None,
        );
        let (collected_secret_shares, collected_pops): (HashMap<_, _>, HashMap<_, _>) =
            received_messages
                .into_iter()
                .filter_map(|(sender, message)| {
                    if let MessageItem::KeygenSharesMessage(KeygenSharesMessage::SecretShares(
                        received_shares,
                        received_pop,
                    )) = message.message
                    {
                        let sender_index = participants.get(&sender).expect("sender exists");
                        Some((
                            (sender_index, received_shares[setup.our_index].clone()),
                            (sender_index, received_pop),
                        ))
                    } else {
                        None
                    }
                })
                .unzip();
        info!("Received secret shares!");
        info!(" \n\n");
        (collected_secret_shares, collected_pops)
    };
    debug!("{}", collected_secret_shares.len());

    let (my_secret_share, frost_key) = frost
        .finish_keygen(
            keygen,
            setup.our_index,
            (0..setup.n_parties)
                .map(|i| {
                    collected_secret_shares
                        .get(&i)
                        .expect("received from party")
                        .clone()
                })
                .collect(),
            (0..setup.n_parties)
                .map(|i| collected_pops.get(&i).expect("received from party").clone())
                .collect(),
        )
        .unwrap();

    debug!("{:?}", &frost_key);
    (my_secret_share, frost_key)
}
