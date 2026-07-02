//! Live integration test for the recovery lobby: 3 participants over
//! nostr, each posting a `SharePost` derived from a fixture ShareBackup,
//! leader publishing `Finish`, and the recovery converging on the
//! same `AccessStructureRef` the original keygen produced.

use frostsnap_coordinator::Sink;
use frostsnap_core::coordinator::restoration::RecoveringAccessStructure;
use frostsnap_core::coordinator::FrostCoordinator;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::test::{
    RunSingleCoordinator as Run, TestEnv, TEST_ENCRYPTION_KEY, TEST_FINGERPRINT,
};
use frostsnap_core::DeviceId;
use frostsnap_nostr::keygen::DeviceKind;
use frostsnap_nostr::recovery::{
    RecoveryChannelMetadata, RecoveryLobbyClient, RecoveryLobbyEvent, RecoveryLobbyHandle,
    SharePost,
};
use frostsnap_nostr::{channel::ChannelSecret, EventId, NostrIdentity, Nsec, PublicKey};
use nostr_relay_builder::prelude::*;
use nostr_sdk::Client;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Clone)]
struct TaggedSink {
    index: usize,
    tx: mpsc::Sender<(usize, RecoveryLobbyEvent)>,
}

impl Sink<RecoveryLobbyEvent> for TaggedSink {
    fn send(&self, event: RecoveryLobbyEvent) {
        let _ = self.tx.try_send((self.index, event));
    }
}

struct Participant {
    #[allow(dead_code)]
    client: Client,
    identity: NostrIdentity,
    handle: RecoveryLobbyHandle,
}

/// End-to-end: leader + 2 joiners each post a `SharePost` built from
/// a fixture share, leader publishes `Finish(winning_refs)`, non-
/// leaders observe `Finished` with an access_structure_ref matching
/// the original keygen.
#[tokio::test]
async fn three_participant_recovery_convergence() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::from_seed([7u8; 32]);
    let mut env = TestEnv::default();

    // --- Step 1: build fixture backups from a real keygen.
    // 3-of-3 keygen so we don't need to worry about which subset to
    // pick — every share is required for reconstruction.
    let mut run = Run::start_after_keygen_and_nonces(3, 3, &mut env, &mut rng, 2, KeyPurpose::Test);
    let asref = run
        .coordinator
        .iter_keys()
        .next()
        .unwrap()
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    for &device_id in &run.device_set() {
        let msgs = run
            .coordinator
            .request_device_display_backup(device_id, asref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(msgs);
    }
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let mut fixture_backups: Vec<_> = env
        .backups
        .iter()
        .map(|(dev, (_name, backup))| (*dev, backup.clone()))
        .collect();
    fixture_backups.sort_by_key(|(_, b)| b.share_image().index);

    // --- Step 2: nostr wiring. Mock relay + 3 participants.
    let relay = MockRelay::run().await.expect("failed to start mock relay");
    let relay_url = relay.url().await;
    let channel_secret = ChannelSecret::random(&mut rng);
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, RecoveryLobbyEvent)>(256);

    let mut participants: Vec<Participant> = Vec::with_capacity(3);
    for i in 0..3 {
        let client = Client::builder().build();
        client.add_relay(&relay_url).await.unwrap();
        client.connect().await;

        let identity = NostrIdentity::Generated {
            nsec: Nsec::generate(),
            name: format!("participant-{i}"),
            created_at: 1_700_000_000 + i as u64,
        };

        let lobby_client =
            RecoveryLobbyClient::new(channel_secret.clone()).with_fingerprint(TEST_FINGERPRINT);
        let (lobby_client, init_event) = if i == 0 {
            let meta = RecoveryChannelMetadata {
                key_name: "recovery-test".into(),
                purpose: KeyPurpose::Test,
                threshold_hint: Some(3),
            };
            let with_meta = lobby_client.with_metadata(meta);
            let init = with_meta.build_creation_event(&identity).await.unwrap();
            (with_meta, Some(init))
        } else {
            (lobby_client, None)
        };
        let sink = TaggedSink {
            index: i,
            tx: event_tx.clone(),
        };
        let handle = lobby_client
            .run(client.clone(), identity.clone(), init_event, sink)
            .await
            .unwrap();
        participants.push(Participant {
            client,
            identity,
            handle,
        });
    }

    // Let the ChannelCreation propagate.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // --- Step 3: each participant posts a SharePost.
    let mut share_event_ids: Vec<EventId> = Vec::with_capacity(3);
    for (i, participant) in participants.iter().enumerate() {
        let (device_id, backup) = &fixture_backups[i];
        let post = SharePost {
            device_id: *device_id,
            device_name: format!("device-{i}"),
            device_kind: DeviceKind::Frostsnap,
            share_image: backup.share_image(),
            needs_consolidation: true,
        };
        let outcome = participant.handle.post_share(post).await.unwrap();
        share_event_ids.push(outcome.inner_event_id);
    }
    // Let all Shares fold on every participant.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // --- Step 4: leader observes RecoveryAvailable → publishes Finish.
    // Track each participant's latest member block (rides every
    // StateChanged snapshot) for the Step-7 profile regression.
    let mut latest_members: Vec<Vec<frostsnap_nostr::GroupMember>> = vec![Vec::new(); 3];
    let winning: Arc<Mutex<Option<Vec<EventId>>>> = Default::default();
    let winning_clone = winning.clone();

    // Drain events until leader sees RecoveryAvailable, capture its
    // winning refs, then Finish.
    let recovery_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < recovery_deadline {
        let Ok(Some((idx, ev))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), event_rx.recv()).await
        else {
            continue;
        };
        if let RecoveryLobbyEvent::StateChanged(snapshot) = &ev {
            latest_members[idx] = snapshot.members.clone();
        }
        if idx == 0 {
            if let RecoveryLobbyEvent::RecoveryAvailable(recovered) = ev {
                *winning_clone.lock().unwrap() = Some(recovered.winning_share_refs);
                break;
            }
        }
    }
    let winning = winning.lock().unwrap().clone();
    let winning = winning.expect("leader must see RecoveryAvailable within 10s");
    assert!(!winning.is_empty(), "winning share_refs must be non-empty");
    participants[0]
        .handle
        .finish(winning.clone())
        .await
        .unwrap();

    // --- Step 5: wait for Finished on all participants — capture
    // the transport-carried bundle (RAS + key_name + purpose) so we
    // can drive a fresh coordinator through finalize_remote_recovery
    // (the same path RemoteRecoveryLobbyHandle::persist_recovered
    // takes at the FFI layer).
    #[derive(Clone)]
    struct FinishedBundle {
        finished: frostsnap_nostr::recovery::FinishedRecovery,
        ras: RecoveringAccessStructure,
        key_name: String,
        purpose: KeyPurpose,
        device_id: DeviceId,
    }
    let mut bundles: Vec<(usize, FinishedBundle)> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline && bundles.len() < 3 {
        match tokio::time::timeout(std::time::Duration::from_millis(500), event_rx.recv()).await {
            Ok(Some((idx, RecoveryLobbyEvent::Finished(finished, ras, key_name, purpose)))) => {
                let (device_id, _) = &fixture_backups[idx];
                bundles.push((
                    idx,
                    FinishedBundle {
                        finished,
                        ras,
                        key_name,
                        purpose,
                        device_id: *device_id,
                    },
                ));
            }
            Ok(Some((idx, RecoveryLobbyEvent::StateChanged(snapshot)))) => {
                latest_members[idx] = snapshot.members;
            }
            Ok(Some(_)) => {}
            _ => {}
        }
    }
    assert_eq!(
        bundles.len(),
        3,
        "all 3 participants should observe Finished"
    );
    for (_, b) in &bundles {
        assert_eq!(
            b.finished.access_structure_ref, asref,
            "recovered access_structure_ref must match original keygen",
        );
    }

    // --- Step 6: each participant drives a fresh FrostCoordinator
    // through finalize_remote_recovery, using the transport-carried
    // bundle. This is what persist_recovered does at the FRB layer
    // (bypassing the FfiCoordinator harness since that's an FFI-only
    // concern; the semantic under test is that the RAS + metadata
    // reach a caller that can finalize with them).
    for (idx, b) in &bundles {
        let mut fresh_coord = FrostCoordinator::new();
        let my_local: BTreeSet<DeviceId> = [b.device_id].into_iter().collect();
        let recovered_asref = fresh_coord
            .finalize_remote_recovery(
                &b.ras,
                b.key_name.clone(),
                b.purpose,
                &my_local,
                TEST_ENCRYPTION_KEY,
                &mut rng,
            )
            .expect("finalize succeeds on fresh coordinator");
        assert_eq!(
            recovered_asref, asref,
            "fresh coordinator {idx} recovered a different access_structure_ref",
        );
        // The just-recovered access structure must be visible on the
        // fresh coordinator — proves the mutation landed.
        assert!(
            fresh_coord.get_access_structure(recovered_asref).is_some(),
            "fresh coordinator {idx} missing access structure after finalize",
        );
    }
    // --- Step 7 (regression): every participant's member surface
    // carries BOTH the leader's profile and its own — the walkthrough
    // bug (joiners rendering pubkeys because pre-creation profile
    // events were dropped by the lobby's metadata gate) is
    // structurally impossible under the block pattern: the member
    // block rides every snapshot and the stash has no gate.
    let has_name = |members: &[frostsnap_nostr::GroupMember], name: &str| {
        members
            .iter()
            .any(|m| m.profile.as_ref().and_then(|p| p.name.as_deref()) == Some(name))
    };
    let names_settled = |latest: &[Vec<frostsnap_nostr::GroupMember>]| {
        (0..3).all(|i| {
            has_name(&latest[i], "participant-0")
                && has_name(&latest[i], &format!("participant-{i}"))
        })
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !names_settled(&latest_members) {
        match tokio::time::timeout(std::time::Duration::from_millis(500), event_rx.recv()).await {
            Ok(Some((idx, RecoveryLobbyEvent::StateChanged(snapshot)))) => {
                latest_members[idx] = snapshot.members;
            }
            Ok(Some(_)) => {}
            _ => {}
        }
    }
    for i in 0..3 {
        assert!(
            has_name(&latest_members[i], "participant-0"),
            "participant {i} never saw the leader's profile in its member block",
        );
        assert!(
            has_name(&latest_members[i], &format!("participant-{i}")),
            "participant {i} never saw its OWN profile in its member block",
        );
    }

    // Sanity: identity of each participant is not empty (touches the
    // NostrIdentity field so it's clearly load-bearing).
    for p in &participants {
        assert!(p.identity.public_key().is_ok());
    }
    tracing::info!("recovery lobby converged + persisted across 3 participants!");
}

/// A leader who publishes `Finish` with a bogus subset (swapped
/// share_image bytes) triggers `FinishVerificationFailed` on non-
/// leaders — the RecoveringAccessStructure::new call on those
/// share_images doesn't reconstruct.
#[tokio::test]
async fn bogus_finish_yields_verification_failed() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::from_seed([13u8; 32]);
    let mut env = TestEnv::default();

    let mut run = Run::start_after_keygen_and_nonces(3, 3, &mut env, &mut rng, 2, KeyPurpose::Test);
    let asref = run
        .coordinator
        .iter_keys()
        .next()
        .unwrap()
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    for &device_id in &run.device_set() {
        let msgs = run
            .coordinator
            .request_device_display_backup(device_id, asref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(msgs);
    }
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let mut fixture_backups: Vec<_> = env
        .backups
        .iter()
        .map(|(dev, (_name, backup))| (*dev, backup.clone()))
        .collect();
    fixture_backups.sort_by_key(|(_, b)| b.share_image().index);

    let relay = MockRelay::run().await.expect("mock relay");
    let relay_url = relay.url().await;
    let channel_secret = ChannelSecret::random(&mut rng);
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, RecoveryLobbyEvent)>(256);

    let mut participants: Vec<Participant> = Vec::with_capacity(2);
    for i in 0..2 {
        let client = Client::builder().build();
        client.add_relay(&relay_url).await.unwrap();
        client.connect().await;
        let identity = NostrIdentity::Generated {
            nsec: Nsec::generate(),
            name: format!("p-{i}"),
            created_at: 1_700_000_000 + i as u64,
        };
        let lobby_client =
            RecoveryLobbyClient::new(channel_secret.clone()).with_fingerprint(TEST_FINGERPRINT);
        let (lobby_client, init_event) = if i == 0 {
            let meta = RecoveryChannelMetadata {
                key_name: "bogus-test".into(),
                purpose: KeyPurpose::Test,
                threshold_hint: Some(3),
            };
            let with_meta = lobby_client.with_metadata(meta);
            let init = with_meta.build_creation_event(&identity).await.unwrap();
            (with_meta, Some(init))
        } else {
            (lobby_client, None)
        };
        let sink = TaggedSink {
            index: i,
            tx: event_tx.clone(),
        };
        let handle = lobby_client
            .run(client.clone(), identity.clone(), init_event, sink)
            .await
            .unwrap();
        participants.push(Participant {
            client,
            identity,
            handle,
        });
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Both post their legitimate share, then leader posts a BOGUS post
    // (swapped share_image bytes) and Finish-refs to it.
    let mut share_event_ids = Vec::new();
    for i in 0..2 {
        let (device_id, backup) = &fixture_backups[i];
        let post = SharePost {
            device_id: *device_id,
            device_name: format!("d-{i}"),
            device_kind: DeviceKind::Frostsnap,
            share_image: backup.share_image(),
            needs_consolidation: true,
        };
        let outcome = participants[i].handle.post_share(post).await.unwrap();
        share_event_ids.push(outcome.inner_event_id);
    }

    // Leader publishes a bogus SharePost — same device_id/index but
    // scrambled share_image bytes.
    let (bogus_device_id, backup) = &fixture_backups[2];
    // Corrupt: keep the legitimate share_image but pair it with a
    // DIFFERENT participant's index. This guarantees `shared_key
    // .share_image(index) != share_image` at Finish verification
    // time — the point-of-index-i differs from the point-of-index-j
    // for a well-formed shared key.
    let mut bogus_image = fixture_backups[0].1.share_image();
    bogus_image.index = backup.share_image().index;
    let bogus_post = SharePost {
        device_id: *bogus_device_id,
        device_name: "bogus".into(),
        device_kind: DeviceKind::Frostsnap,
        share_image: bogus_image,
        needs_consolidation: true,
    };
    let bogus_outcome = participants[0].handle.post_share(bogus_post).await.unwrap();
    share_event_ids.push(bogus_outcome.inner_event_id);
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    participants[0]
        .handle
        .finish(share_event_ids.clone())
        .await
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut saw_verification_failed = false;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(500), event_rx.recv()).await {
            Ok(Some((_, RecoveryLobbyEvent::FinishVerificationFailed))) => {
                saw_verification_failed = true;
                break;
            }
            Ok(Some((_, RecoveryLobbyEvent::Finished(..)))) => {
                panic!("bogus finish must not verify");
            }
            _ => {}
        }
    }
    assert!(
        saw_verification_failed,
        "expected FinishVerificationFailed within 10s",
    );
}
