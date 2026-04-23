//! Live integration test: the lobby + protocol keygen flow over a nostr
//! MockRelay, driven by a single `Run` that holds every participant's
//! coordinator + devices. Each participant runs an independent `LobbyClient`;
//! when the lobby resolves the keygen, the test spins up a per-participant
//! `ProtocolClient` and routes FROST messages through its handle.

use frostsnap_coordinator::Sink;
use frostsnap_core::coordinator::remote_keygen::RemoteKeygenMessage;
use frostsnap_core::coordinator::BroadcastPayload;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
use frostsnap_core::test::Run;
use frostsnap_core::KeygenId;
use frostsnap_nostr::{
    channel::ChannelSecret,
    keygen::{
        DeviceKind, DeviceRegistration, LobbyClient, LobbyEvent, LobbyHandle, LobbyState,
        ParticipantStatus, ProtocolClient, ProtocolHandle, SelectedCoordinator,
    },
};
use nostr_relay_builder::prelude::*;
use nostr_sdk::{Client, EventId, Keys};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use std::collections::BTreeSet;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
enum TestEvent {
    Lobby(LobbyEvent),
    Keygen(RemoteKeygenMessage),
}

#[derive(Clone)]
struct TaggedLobbySink {
    index: usize,
    tx: mpsc::Sender<(usize, TestEvent)>,
}

impl Sink<LobbyEvent> for TaggedLobbySink {
    fn send(&self, event: LobbyEvent) {
        let _ = self.tx.try_send((self.index, TestEvent::Lobby(event)));
    }
}

#[derive(Clone)]
struct TaggedKeygenSink {
    index: usize,
    tx: mpsc::Sender<(usize, TestEvent)>,
}

impl Sink<RemoteKeygenMessage> for TaggedKeygenSink {
    fn send(&self, msg: RemoteKeygenMessage) {
        let _ = self.tx.try_send((self.index, TestEvent::Keygen(msg)));
    }
}

struct NostrSide {
    client: Client,
    nostr_keys: Keys,
    lobby_handle: LobbyHandle,
    protocol_handle: Option<ProtocolHandle>,
}

#[tokio::test]
async fn two_coordinators_one_device_each() {
    run_keygen_test(&[1, 1], &[0, 1], 2, 42).await;
}

#[tokio::test]
async fn three_coordinators_one_with_two_devices() {
    run_keygen_test(&[1, 1, 2], &[0, 1, 2], 2, 123).await;
}

#[tokio::test]
async fn three_coordinators_two_selected_private_protocol() {
    run_keygen_test(&[1, 1, 1], &[0, 1], 2, 999).await;
}

async fn run_keygen_test(
    device_counts: &[usize],
    selected_participants: &[usize],
    threshold: u16,
    seed: u64,
) {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let relay = MockRelay::run().await.expect("failed to start relay");
    let relay_url = relay.url().await;
    let channel_secret = ChannelSecret::random(&mut rng);
    let n_coordinators = device_counts.len();
    let selected_participants: BTreeSet<usize> = selected_participants.iter().copied().collect();

    let mut run = Run::generate_remote(device_counts, &mut rng).with_manual_broadcast_routing();

    // Override each participant's coordinator keypair with a nostr-derived one
    // so nostr_pubkey_to_device_id produces the matching coordinator ID.
    let mut nostr_keys_by_index: Vec<Keys> = Vec::with_capacity(n_coordinators);
    for p in run.participants.iter_mut() {
        let nostr_keys = Keys::generate();
        let scalar = Scalar::from_bytes(nostr_keys.secret_key().secret_bytes())
            .expect("nostr secret key is a valid scalar")
            .non_zero()
            .expect("nostr secret key is non-zero");
        p.keypair = KeyPair::new_xonly(scalar).into();
        nostr_keys_by_index.push(nostr_keys);
    }

    // Start a LobbyClient per participant.
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, TestEvent)>(256);
    let mut nostr_sides: Vec<NostrSide> = Vec::with_capacity(n_coordinators);
    for (i, nostr_keys) in nostr_keys_by_index.iter().enumerate() {
        let client = Client::builder().build();
        client.add_relay(&relay_url).await.unwrap();
        client.connect().await;

        let lobby_client = LobbyClient::new(channel_secret.clone());
        let init_event = if i == 0 {
            Some(lobby_client.build_creation_event(nostr_keys).await.unwrap())
        } else {
            None
        };

        let sink = TaggedLobbySink {
            index: i,
            tx: event_tx.clone(),
        };

        let lobby_handle = lobby_client
            .run(client.clone(), nostr_keys.clone(), init_event, sink)
            .await
            .unwrap();
        nostr_sides.push(NostrSide {
            client,
            nostr_keys: nostr_keys.clone(),
            lobby_handle,
            protocol_handle: None,
        });
    }

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Register devices.
    let mut register_event_ids = Vec::new();
    for (i, side) in nostr_sides.iter().enumerate() {
        let regs: Vec<DeviceRegistration> = run.participants[i]
            .devices
            .keys()
            .enumerate()
            .map(|(j, id)| DeviceRegistration {
                device_id: *id,
                name: format!("device-{i}-{j}"),
                kind: DeviceKind::Frostsnap,
            })
            .collect();
        let eid = side
            .lobby_handle
            .register_devices(&side.nostr_keys, regs)
            .await
            .unwrap();
        register_event_ids.push(eid);
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Set wallet name + propose threshold + start keygen (initiator = participant 0).
    nostr_sides[0]
        .lobby_handle
        .set_key_name(
            &nostr_sides[0].nostr_keys,
            "test-key".into(),
            KeyPurpose::Test,
        )
        .await
        .unwrap();
    nostr_sides[0]
        .lobby_handle
        .set_threshold(&nostr_sides[0].nostr_keys, threshold)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let selected_coordinators: Vec<SelectedCoordinator> = selected_participants
        .iter()
        .map(|&i| SelectedCoordinator {
            register_event_id: register_event_ids[i],
            pubkey: nostr_sides[i].nostr_keys.public_key(),
        })
        .collect();
    nostr_sides[0]
        .lobby_handle
        .start_keygen(
            &nostr_sides[0].nostr_keys,
            &selected_coordinators,
            threshold,
            "test-key".into(),
            KeyPurpose::Test,
        )
        .await
        .unwrap();

    // Main test loop: drive the event pump until no events arrive for a while.
    let mut keygen_event_id: Option<EventId> = None;
    let mut keygen_messages_seen = vec![0usize; n_coordinators];

    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv()).await {
            Ok(Some((
                i,
                TestEvent::Lobby(LobbyEvent::KeygenResolved {
                    resolved,
                    channel_keys,
                }),
            ))) => {
                if nostr_sides[i].protocol_handle.is_some() {
                    continue;
                }
                keygen_event_id = Some(resolved.keygen_event_id);

                let protocol_sink = TaggedKeygenSink {
                    index: i,
                    tx: event_tx.clone(),
                };
                let protocol_handle = ProtocolClient::run(
                    nostr_sides[i].client.clone(),
                    channel_keys,
                    resolved.keygen_event_id,
                    resolved.allowed_senders(),
                    protocol_sink,
                )
                .await
                .unwrap();
                nostr_sides[i].protocol_handle = Some(protocol_handle);

                let coordinator_ids = resolved.coordinator_ids();
                let begin = resolved.to_begin_keygen();
                let p = &mut run.participants[i];
                let local_devices = p.devices.keys().copied().collect();
                let keypair = p.keypair;
                let sends = p
                    .coordinator
                    .begin_remote_keygen(begin, &coordinator_ids, &local_devices, keypair, &mut rng)
                    .unwrap();
                run.extend_from_coordinator(i, sends);
                pump(&nostr_sides, &mut run, &mut rng).await;
            }
            Ok(Some((_, TestEvent::Lobby(LobbyEvent::LobbyChanged(_))))) => continue,
            Ok(Some((_, TestEvent::Lobby(LobbyEvent::Cancelled)))) => continue,
            Ok(Some((i, TestEvent::Keygen(message)))) => {
                keygen_messages_seen[i] += 1;
                let eid = keygen_event_id.expect("keygen started before KeygenMessage arrived");
                run.inject_keygen_message(i, KeygenId(eid.to_bytes()), message)
                    .unwrap();
                pump(&nostr_sides, &mut run, &mut rng).await;
            }
            Ok(None) => break,
            Err(_) => break, // no events for 5s — protocol should be done
        }
    }

    // Verify agreement.
    let participants_with_keys: Vec<_> = run
        .participants
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.coordinator.iter_keys().next().map(|key| (i, key)))
        .collect();
    assert_eq!(
        participants_with_keys.len(),
        selected_participants.len(),
        "only selected coordinators should have the key"
    );
    for (i, _) in &participants_with_keys {
        assert!(
            selected_participants.contains(i),
            "excluded coordinator {i} should not have the key",
        );
    }
    for i in 0..n_coordinators {
        if !selected_participants.contains(&i) {
            assert_eq!(
                keygen_messages_seen[i], 0,
                "excluded coordinator {i} should not receive private keygen messages",
            );
        }
    }
    for (_, key) in participants_with_keys.iter().skip(1) {
        assert_eq!(
            participants_with_keys[0].1.complete_key.master_appkey, key.complete_key.master_appkey,
            "selected coordinators should agree on master_appkey",
        );
    }
    tracing::info!("keygen completed successfully!");
}

/// The host's NIP-28 ChannelCreation event is the canonical "I'm in
/// the lobby" signal — no separate Presence is published. This test
/// asserts that the host appears in `participants` purely from the
/// ChannelCreation pathway, AND that the invariant
/// "initiator set ⇒ participants non-empty" never breaks across the
/// stream of `LobbyChanged` events.
#[tokio::test]
async fn host_sees_self_on_open() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(99);
    let relay = MockRelay::run().await.expect("failed to start relay");
    let relay_url = relay.url().await;
    let channel_secret = ChannelSecret::random(&mut rng);

    let nostr_keys = Keys::generate();
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, TestEvent)>(256);

    let client = Client::builder().build();
    client.add_relay(&relay_url).await.unwrap();
    client.connect().await;

    let lobby_client = LobbyClient::new(channel_secret);
    let init_event = lobby_client
        .build_creation_event(&nostr_keys)
        .await
        .unwrap();
    let sink = TaggedLobbySink {
        index: 0,
        tx: event_tx.clone(),
    };

    let _handle = lobby_client
        .run(client, nostr_keys.clone(), Some(init_event), sink)
        .await
        .unwrap();

    let me = nostr_keys.public_key();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut seen = false;
    while !seen {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, event_rx.recv()).await {
            Ok(Some((_, TestEvent::Lobby(LobbyEvent::LobbyChanged(state))))) => {
                // Invariant: initiator set ⇒ participants non-empty.
                if state.initiator.is_some() {
                    assert!(
                        !state.participants.is_empty(),
                        "every state with `initiator` set must have a non-empty participants map",
                    );
                    assert!(
                        state.participants.contains_key(&state.initiator.unwrap()),
                        "the initiator must be present in participants",
                    );
                }
                if state.participants.contains_key(&me) {
                    seen = true;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(seen, "host should see themselves in participants");
}

/// Full lobby lifecycle: presence on join → register moves to Ready →
/// host proposes threshold → participant accepts → all accepted. Then
/// exercise host kick of a third joiner, participant leave, and host
/// cancel.
#[tokio::test]
async fn lobby_full_lifecycle() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(7);
    let relay = MockRelay::run().await.expect("failed to start relay");
    let relay_url = relay.url().await;
    let channel_secret = ChannelSecret::random(&mut rng);
    let n = 2;

    let nostr_keys: Vec<Keys> = (0..n).map(|_| Keys::generate()).collect();
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, TestEvent)>(256);

    let mut clients: Vec<Client> = Vec::with_capacity(n);
    let mut lobby_handles: Vec<LobbyHandle> = Vec::with_capacity(n);
    for (i, keys) in nostr_keys.iter().enumerate() {
        let client = Client::builder().build();
        client.add_relay(&relay_url).await.unwrap();
        client.connect().await;

        let lobby_client = LobbyClient::new(channel_secret.clone());
        let init_event = if i == 0 {
            Some(lobby_client.build_creation_event(keys).await.unwrap())
        } else {
            None
        };
        let sink = TaggedLobbySink {
            index: i,
            tx: event_tx.clone(),
        };
        let handle = lobby_client
            .run(client.clone(), keys.clone(), init_event, sink)
            .await
            .unwrap();
        clients.push(client);
        lobby_handles.push(handle);
    }

    // Presence lands after `run()` returns — wait for everyone to see everyone.
    let states = collect_lobby_states(&mut event_rx, n, std::time::Duration::from_secs(3), |ss| {
        ss.iter()
            .all(|s| s.as_ref().is_some_and(|s| s.participants.len() == n))
    })
    .await;
    for (i, s) in states.iter().enumerate() {
        let s = s.as_ref().unwrap();
        for p in s.participants.values() {
            assert_eq!(
                p.status,
                ParticipantStatus::Joining,
                "before register, participant {i} should see everyone as Joining",
            );
        }
    }

    // Host publishes the key name + threshold before anyone is Ready — this
    // is the natural ordering in the UI.
    lobby_handles[0]
        .set_key_name(
            &nostr_keys[0],
            "test-key".into(),
            frostsnap_core::device::KeyPurpose::Test,
        )
        .await
        .unwrap();
    lobby_handles[0]
        .set_threshold(&nostr_keys[0], 2)
        .await
        .unwrap();

    // Both participants mark themselves Ready.
    for (i, handle) in lobby_handles.iter().enumerate() {
        let regs = vec![DeviceRegistration {
            device_id: frostsnap_core::DeviceId([i as u8; 33]),
            name: format!("device-{i}"),
            kind: DeviceKind::Frostsnap,
        }];
        handle.register_devices(&nostr_keys[i], regs).await.unwrap();
    }

    let states = collect_lobby_states(&mut event_rx, n, std::time::Duration::from_secs(3), |ss| {
        ss.iter().all(|s| s.as_ref().is_some_and(|s| s.all_ready()))
    })
    .await;
    for s in &states {
        let s = s.as_ref().unwrap();
        assert!(s.all_ready(), "everyone should be Ready");
        assert_eq!(s.threshold, Some(2));
        assert_eq!(s.key_name.as_deref(), Some("test-key"));
    }

    // Every participant accepts the host's threshold.
    for (i, handle) in lobby_handles.iter().enumerate() {
        handle.accept_threshold(&nostr_keys[i], 2).await.unwrap();
    }
    let states = collect_lobby_states(&mut event_rx, n, std::time::Duration::from_secs(3), |ss| {
        ss.iter()
            .all(|s| s.as_ref().is_some_and(|s| s.all_accepted()))
    })
    .await;
    for s in &states {
        assert!(
            s.as_ref().unwrap().all_accepted(),
            "everyone should have Accepted the threshold",
        );
    }

    // Participant 1 leaves — initiator should see them removed.
    lobby_handles[1].leave(&nostr_keys[1]).await.unwrap();
    let states = collect_lobby_states(&mut event_rx, n, std::time::Duration::from_secs(3), |ss| {
        ss[0].as_ref().is_some_and(|s| s.participants.len() == 1)
    })
    .await;
    let host_view = states[0].as_ref().unwrap();
    assert_eq!(host_view.participants.len(), 1);
    assert!(host_view
        .participants
        .contains_key(&nostr_keys[0].public_key()));

    // Initiator cancels.
    lobby_handles[0].cancel_lobby(&nostr_keys[0]).await.unwrap();
    let mut saw_cancelled = vec![false; n];
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while !saw_cancelled.iter().all(|seen| *seen) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, event_rx.recv()).await {
            Ok(Some((i, TestEvent::Lobby(LobbyEvent::Cancelled)))) => saw_cancelled[i] = true,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(
        saw_cancelled.iter().all(|seen| *seen),
        "all participants should see Cancelled: {saw_cancelled:?}",
    );
}

async fn collect_lobby_states<F>(
    event_rx: &mut mpsc::Receiver<(usize, TestEvent)>,
    n: usize,
    timeout: std::time::Duration,
    predicate: F,
) -> Vec<Option<LobbyState>>
where
    F: Fn(&[Option<LobbyState>]) -> bool,
{
    let mut latest: Vec<Option<LobbyState>> = vec![None; n];
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if predicate(&latest) {
            return latest;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return latest;
        }
        match tokio::time::timeout(remaining, event_rx.recv()).await {
            Ok(Some((i, TestEvent::Lobby(LobbyEvent::LobbyChanged(state))))) => {
                latest[i] = Some(state);
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return latest,
        }
    }
}

/// Drain any outbound broadcasts produced by the coordinators (send them over
/// the protocol subchannel), then drive the default `Env` until the in-memory
/// queue is empty.
async fn pump(nostr_sides: &[NostrSide], run: &mut Run, rng: &mut ChaCha20Rng) {
    use frostsnap_core::test::Env;
    struct DefaultEnv;
    impl Env for DefaultEnv {}

    loop {
        let outbound: Vec<_> = run.drain_outbound_broadcasts().into_iter().collect();
        let queue_empty = run.message_queue.is_empty();
        if outbound.is_empty() && queue_empty {
            return;
        }
        for ob in outbound {
            let side = &nostr_sides[ob.coordinator_index];
            let BroadcastPayload::RemoteKeygen(payload) = ob.payload;
            let protocol = side
                .protocol_handle
                .as_ref()
                .expect("protocol channel must be started before broadcasting keygen messages");
            protocol
                .send_keygen_payload(&side.nostr_keys, ob.from, payload)
                .await
                .unwrap();
        }
        run.run_until_finished(&mut DefaultEnv, rng).unwrap();
    }
}
