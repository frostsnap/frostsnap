//! Live integration test: coordinators run a threshold signing round over a
//! nostr MockRelay using the flat-CRDT + settling-timer protocol. Keygen +
//! nonce replenishment happen in-memory via `Run`; only the signing exchange
//! (Request → flat Offers → TimerExpired → RoundConfirmed → Partials)
//! travels through nostr. After the partials land we combine them off-band
//! and verify the final Schnorr signature.

use bitcoin::{Network, Witness};
use frostsnap_coordinator::Sink;
use frostsnap_core::bitcoin_transaction::TransactionTemplate;
use frostsnap_core::coordinator::remote_signing::SignOffer;
use frostsnap_core::coordinator::signing::{ParticipantSignatureShares, RemoteSignSessionId};
use frostsnap_core::coordinator::{
    remote_signing, CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage,
    CoordinatorToUserSigningMessage,
};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::schnorr_fun::Schnorr;
use frostsnap_core::silent_payments::SilentPaymentRecipient;
use frostsnap_core::test::silent_payments::{
    bitcoin_template, expected_output_keys, public, recipient, reconstruct_root_secret, scalar,
    template,
};
use frostsnap_core::test::{Env, Run, TEST_ENCRYPTION_KEY};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::MasterAppkey;
use frostsnap_core::{SignSessionId, WireSignTask};
use frostsnap_nostr::channel::ChannelInitData;
use frostsnap_nostr::signing::{ChannelClient, ChannelEvent, ConfirmedSubsetEntry, SigningEvent};
use frostsnap_nostr::EventId;
use frostsnap_nostr::{NostrIdentity, Nsec, ToBech32};
use nostr_relay_builder::prelude::*;
use nostr_sdk::{Client, Keys};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use tokio::sync::mpsc;

const TEST_SETTLING_WINDOW: Duration = Duration::from_millis(400);

#[derive(Clone)]
struct TaggedSink {
    index: usize,
    tx: mpsc::Sender<(usize, ChannelEvent)>,
}

impl Sink<ChannelEvent> for TaggedSink {
    fn send(&self, event: ChannelEvent) {
        let _ = self.tx.try_send((self.index, event));
    }
}

#[derive(Default)]
struct SigningEnv {
    shares: BTreeMap<(usize, SignSessionId), Vec<ParticipantSignatureShares>>,
}

impl Env for SigningEnv {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        ci: usize,
        message: CoordinatorToUserMessage,
        rng: &mut impl rand_chacha::rand_core::RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner:
                    CoordinatorToUserKeyGenMessage::KeyGenAck {
                        all_acks_received: true,
                        ..
                    },
            } => {
                let sends = run.participants[ci]
                    .coordinator
                    .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                    .unwrap();
                run.extend_from_coordinator(ci, sends);
            }
            CoordinatorToUserMessage::Signing(CoordinatorToUserSigningMessage::GotShare {
                session_id,
                shares,
                ..
            }) => {
                self.shares
                    .entry((ci, session_id))
                    .or_default()
                    .push(shares);
            }
            _ => {}
        }
    }
}

struct NostrSide {
    keys: Keys,
    handle: frostsnap_nostr::ChannelHandle,
}

#[tokio::test]
async fn two_coordinators_sign_over_nostr() {
    run_signing_test(&[1, 1], 2, 42).await;
}

#[tokio::test]
async fn three_coordinators_threshold_two_sign_over_nostr() {
    run_signing_test(&[1, 1, 1], 2, 1234).await;
}

#[tokio::test]
async fn round_stays_pending_with_insufficient_offers() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(9);
    let mut env = SigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1],
        2,
        "abort key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let (key_context, init_data, _key_data) = extract_key_artifacts(&run);
    let (relay_url, _relay) = start_relay().await;
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, ChannelEvent)>(512);
    let sides = spawn_sides(&run, 2, &relay_url, &key_context, &init_data, event_tx).await;
    settle_subscriptions().await;

    let sign_task = WireSignTask::Test {
        message: "aborted sign".into(),
    };
    let request_id = sides[0]
        .handle
        .send_sign_request(&sides[0].keys, sign_task.clone(), "please".into())
        .await
        .unwrap();
    wait_for_request(&mut event_rx, 2, request_id).await;

    // Only one side offers — short of the threshold.
    let ar = extract_ar(&run);
    let device_id = *run.participants[0].devices.keys().next().unwrap();
    let offer = run.participants[0]
        .coordinator
        .offer_to_sign(
            RemoteSignSessionId::new([0u8; 32]),
            ar,
            sign_task.clone(),
            device_id,
        )
        .unwrap();
    sides[0]
        .handle
        .send_sign_offer(&sides[0].keys, request_id, vec![offer.participant_binonces])
        .await
        .unwrap();

    let pendings = wait_for_round_pending(&mut event_rx, 2, request_id).await;
    for pending in &pendings {
        assert_eq!(pending.observed.len(), 1);
        assert_eq!(pending.threshold, 2);
    }
}

#[tokio::test]
async fn deterministic_subset_regardless_of_send_order() {
    // 3 participants, threshold 2. Offers published in various orders; every
    // participant's RoundConfirmed must carry the same subset + session_id.
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(99);
    let mut env = SigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1, 1],
        2,
        "det key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let (key_context, init_data, _key_data) = extract_key_artifacts(&run);
    let (relay_url, _relay) = start_relay().await;
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, ChannelEvent)>(512);
    let sides = spawn_sides(&run, 3, &relay_url, &key_context, &init_data, event_tx).await;
    settle_subscriptions().await;

    let sign_task = WireSignTask::Test {
        message: "det sign".into(),
    };
    let request_id = sides[0]
        .handle
        .send_sign_request(&sides[0].keys, sign_task.clone(), "please".into())
        .await
        .unwrap();
    wait_for_request(&mut event_rx, 3, request_id).await;

    let nonce_reservation_id = RemoteSignSessionId::new([7u8; 32]);
    let ar = extract_ar(&run);

    // Publish offers in reverse-participant order (2, 1, 0). The selector
    // sorts by (timestamp, event_id), so the final subset is independent of
    // publish order.
    for i in [2usize, 1, 0] {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let offer = run.participants[i]
            .coordinator
            .offer_to_sign(nonce_reservation_id, ar, sign_task.clone(), device_id)
            .unwrap();
        sides[i]
            .handle
            .send_sign_offer(&sides[i].keys, request_id, vec![offer.participant_binonces])
            .await
            .unwrap();
        // Tiny stagger so event timestamps can differ in the second resolution.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let confirmations = wait_for_round_confirmed(&mut event_rx, 3, request_id).await;
    let first_session_id = confirmations[0].session_id;
    let first_subset_ids: Vec<EventId> =
        confirmations[0].subset.iter().map(|e| e.event_id).collect();
    for c in &confirmations[1..] {
        assert_eq!(c.session_id, first_session_id, "session_id disagreement");
        let subset_ids: Vec<EventId> = c.subset.iter().map(|e| e.event_id).collect();
        assert_eq!(subset_ids, first_subset_ids, "subset disagreement");
    }
    assert_eq!(first_subset_ids.len(), 2);

    // Deterministic order property: the selector sorts ascending by
    // (timestamp, event_id), so each subsequent entry should compare ≥ the
    // previous one.
    for pair in confirmations[0].subset.windows(2) {
        assert!(
            (pair[0].timestamp, pair[0].event_id) <= (pair[1].timestamp, pair[1].event_id),
            "subset not in deterministic order",
        );
    }
}

async fn run_signing_test(device_counts: &[usize], threshold: u16, seed: u64) {
    let _ = tracing_subscriber::fmt::try_init();
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let mut env = SigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        device_counts,
        threshold,
        "nostr sign key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let n = run.participants.len();
    let t = threshold as usize;

    let (key_context, init_data, key_data) = extract_key_artifacts(&run);
    let access_structure_ref = extract_ar(&run);
    let (relay_url, _relay) = start_relay().await;
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, ChannelEvent)>(512);
    let sides = spawn_sides(&run, n, &relay_url, &key_context, &init_data, event_tx).await;
    settle_subscriptions().await;

    let sign_task = WireSignTask::Test {
        message: "nostr remote signing test".into(),
    };
    let request_id = sides[0]
        .handle
        .send_sign_request(&sides[0].keys, sign_task.clone(), "sign please".into())
        .await
        .unwrap();
    wait_for_request(&mut event_rx, n, request_id).await;

    // All `t` eligible participants publish offers — the settling window
    // accumulates them into a flat set under the Request. Publishes are
    // fast relative to the settling window, so sequential iteration
    // produces the same observable behaviour as concurrent publishing.
    let nonce_reservation_id = RemoteSignSessionId::new([0u8; 32]);
    let mut offered_pairs: Vec<(usize, _)> = Vec::with_capacity(t);
    for i in 0..t {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let offer = run.participants[i]
            .coordinator
            .offer_to_sign(
                nonce_reservation_id,
                access_structure_ref,
                sign_task.clone(),
                device_id,
            )
            .unwrap();
        offered_pairs.push((i, offer.participant_binonces.clone()));
        sides[i]
            .handle
            .send_sign_offer(&sides[i].keys, request_id, vec![offer.participant_binonces])
            .await
            .unwrap();
    }

    // Wait for every participant's RoundConfirmed. The settling timer drives
    // the tree from collecting to confirmed once the stream goes quiet.
    let confirmations = wait_for_round_confirmed(&mut event_rx, n, request_id).await;
    let confirmed = &confirmations[0];
    for c in &confirmations[1..] {
        assert_eq!(c.session_id, confirmed.session_id);
    }
    let session_id = confirmed.session_id;
    let binonces_for_signing: Vec<_> = confirmed
        .subset
        .iter()
        .flat_map(|e| e.binonces.iter().cloned())
        .collect();
    let subset_authors: BTreeSet<_> = confirmed.subset.iter().map(|e| e.author).collect();
    assert_eq!(confirmed.subset.len(), t);
    assert!(
        confirmed.sp.is_none(),
        "a plain send carries no silent payment subset"
    );

    // Each included participant signs with the confirmed binonce set.
    for (i, _) in &offered_pairs {
        assert!(
            subset_authors.contains(&frostsnap_nostr::PublicKey::from(
                sides[*i].keys.public_key()
            )),
            "coordinator {i} was expected to be included in the subset",
        );
        let device_id = *run.participants[*i].devices.keys().next().unwrap();
        let sign_req = run.participants[*i]
            .coordinator
            .sign_with_nonce_reservation(
                nonce_reservation_id,
                device_id,
                &binonces_for_signing,
                None,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        run.extend_from_coordinator(*i, sign_req);
    }
    run.run_until_finished(&mut env, &mut rng).unwrap();

    assert_eq!(
        env.shares.len(),
        t,
        "each signing coordinator should have emitted one GotShare"
    );

    let offer_subset_ids: Vec<EventId> = confirmed.subset.iter().map(|e| e.event_id).collect();
    let mut all_shares: Vec<ParticipantSignatureShares> = Vec::with_capacity(t);
    for (i, _) in &offered_pairs {
        let shares = env.shares[&(*i, session_id)][0].clone();
        sides[*i]
            .handle
            .send_sign_partial(
                &sides[*i].keys,
                request_id,
                offer_subset_ids.clone(),
                shares.clone(),
            )
            .await
            .unwrap();
        all_shares.push(shares);
    }
    wait_for_partials(&mut event_rx, n, t, request_id).await;

    let all_shares_refs: Vec<_> = all_shares.iter().collect();
    let signatures = remote_signing::combine_signatures(
        confirmed.sign_task.clone(),
        &key_context,
        &binonces_for_signing,
        &all_shares_refs,
    )
    .unwrap();

    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let checked_task = confirmed
        .sign_task
        .clone()
        .check(key_data.complete_key.master_appkey, KeyPurpose::Test)
        .unwrap();
    let decoded: Vec<_> = signatures
        .iter()
        .map(|s| (*s).into_decoded().unwrap())
        .collect();
    assert!(
        checked_task.verify_final_signatures(&schnorr, &decoded),
        "combined signatures should verify",
    );
    tracing::info!("nostr remote signing completed successfully");
}
#[tokio::test]
async fn three_coordinators_silent_payment_send_over_nostr() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut fx = prepare_sp_channel(21).await;
    let offers: Vec<_> = (0..3).map(|i| offer_sp(&mut fx, i)).collect();
    for offer in &offers {
        assert_eq!(offer.sp_ecdh.len(), 2, "one share per distinct scan key");
    }
    let mut offer_ids = Vec::new();
    for (i, offer) in offers.iter().enumerate() {
        offer_ids.push(
            fx.sides[i]
                .handle
                .send_sp_sign_offer(
                    &fx.sides[i].keys,
                    fx.request_id,
                    vec![offer.participant_binonces.clone()],
                    offer.sp_ecdh.clone(),
                )
                .await
                .unwrap(),
        );
    }

    let confirmations = wait_for_round_confirmed(&mut fx.event_rx, 3, fx.request_id).await;
    let confirmed = &confirmations[0];
    for c in &confirmations[1..] {
        assert_eq!(c.session_id, confirmed.session_id);
        assert_eq!(c.sp, confirmed.sp);
    }
    finish_sp_round(&mut fx, confirmed, &offer_ids).await;
}

#[tokio::test]
async fn invalid_silent_payment_offer_is_not_selected() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut fx = prepare_sp_channel(22).await;
    // ECDH first, then publish. The tampered offer goes out first, so oldest-first would select
    // it if the tree let a bad proof into the offer set.
    let offers: Vec<_> = (0..3).map(|i| offer_sp(&mut fx, i)).collect();
    let mut tampered = offers[2].sp_ecdh.clone();
    tampered[0].ecdh_share = public(scalar(0x77));
    let bad_id = fx.sides[2]
        .handle
        .send_sp_sign_offer(
            &fx.sides[2].keys,
            fx.request_id,
            vec![offers[2].participant_binonces.clone()],
            tampered,
        )
        .await
        .unwrap();
    let mut honest_ids = Vec::new();
    for (side, offer) in fx.sides.iter().zip(&offers).take(2) {
        honest_ids.push(
            side.handle
                .send_sp_sign_offer(
                    &side.keys,
                    fx.request_id,
                    vec![offer.participant_binonces.clone()],
                    offer.sp_ecdh.clone(),
                )
                .await
                .unwrap(),
        );
    }

    let (confirmations, rejected) =
        wait_for_round_confirmed_and_rejections(&mut fx.event_rx, 3, fx.request_id).await;
    assert!(
        rejected.contains(&bad_id),
        "a tampered ECDH share must be rejected before subset selection"
    );
    let confirmed = &confirmations[0];
    let subset_ids: Vec<_> = confirmed.subset.iter().map(|e| e.event_id).collect();
    assert_eq!(subset_ids.len(), 2);
    assert!(!subset_ids.contains(&bad_id));
    assert_eq!(
        subset_ids.iter().copied().collect::<BTreeSet<_>>(),
        honest_ids.iter().copied().collect::<BTreeSet<_>>(),
    );
    finish_sp_round(&mut fx, confirmed, &honest_ids).await;
}

// ============================================================================
// Helpers
// ============================================================================

fn extract_key_artifacts(
    run: &Run,
) -> (
    frostsnap_core::coordinator::KeyContext,
    ChannelInitData,
    frostsnap_core::coordinator::CoordFrostKey,
) {
    let key_data = run.participants[0]
        .coordinator
        .iter_keys()
        .next()
        .expect("keygen produced a key")
        .clone();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let root_shared_key = key_data
        .complete_key
        .root_shared_key(
            access_structure_ref.access_structure_id,
            TEST_ENCRYPTION_KEY,
        )
        .expect("can decrypt rootkey with test encryption key");
    let participant_devices: Vec<_> = run
        .participants
        .iter()
        .map(|p| {
            let secret_bytes = p.keypair.secret_key().to_bytes();
            let keys = Keys::new(nostr_sdk::SecretKey::from_slice(&secret_bytes).unwrap());
            let pubkey: frostsnap_nostr::PublicKey = keys.public_key().into();
            let devices: Vec<_> = p.devices.keys().copied().collect();
            (pubkey, devices)
        })
        .collect();
    let devices_in_order: Vec<_> = run
        .participants
        .iter()
        .flat_map(|p| p.devices.keys().copied())
        .collect();
    let participants = frostsnap_nostr::channel::ParticipantShares::from_device_lists(
        &participant_devices,
        &devices_in_order,
    )
    .expect("all devices should be in devices_in_order");
    let init_data = ChannelInitData {
        key_name: key_data.key_name.clone(),
        purpose: key_data.purpose,
        root_shared_key,
        participants,
    };
    let key_context = init_data.key_context();
    (key_context, init_data, key_data)
}

fn extract_ar(run: &Run) -> frostsnap_core::AccessStructureRef {
    run.participants[0]
        .coordinator
        .iter_keys()
        .next()
        .unwrap()
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref()
}

async fn start_relay() -> (String, MockRelay) {
    let relay = MockRelay::run().await.expect("failed to start relay");
    let url = relay.url().await;
    (url.to_string(), relay)
}

async fn settle_subscriptions() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn spawn_sides(
    run: &Run,
    n: usize,
    relay_url: &str,
    key_context: &frostsnap_core::coordinator::KeyContext,
    init_data: &ChannelInitData,
    event_tx: mpsc::Sender<(usize, ChannelEvent)>,
) -> Vec<NostrSide> {
    let mut sides = Vec::with_capacity(n);
    for i in 0..n {
        let secret_bytes = run.participants[i].keypair.secret_key().to_bytes();
        let keys = Keys::new(nostr_sdk::SecretKey::from_slice(&secret_bytes).unwrap());
        let client = Client::builder().build();
        client.add_relay(relay_url).await.unwrap();
        client.connect().await;

        let init = if i == 0 {
            Some(init_data.clone())
        } else {
            None
        };
        let channel_client = ChannelClient::new(key_context.clone(), init)
            .with_settling_window(TEST_SETTLING_WINDOW);
        let sink = TaggedSink {
            index: i,
            tx: event_tx.clone(),
        };
        let identity = identity_from_keys(&keys, &format!("signer-{i}"));
        let handle = channel_client.run(client, sink, identity).await.unwrap();
        sides.push(NostrSide { keys, handle });
    }
    drop(event_tx);
    sides
}

/// Wrap raw nostr `Keys` in a `NostrIdentity` for `run()` — tests
/// derive protocol scalars from these exact keys, so we can't just
/// `Nsec::generate()` a fresh identity.
fn identity_from_keys(keys: &Keys, name: &str) -> NostrIdentity {
    NostrIdentity::Generated {
        nsec: Nsec(keys.secret_key().to_bech32().expect("valid key")),
        name: name.to_string(),
        created_at: 1_700_000_000,
    }
}

async fn wait_for_request(
    rx: &mut mpsc::Receiver<(usize, ChannelEvent)>,
    n: usize,
    request_id: EventId,
) {
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    while seen.len() < n {
        let (i, event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for Request")
            .expect("event channel closed");
        if let ChannelEvent::Signing {
            event: SigningEvent::Request { event_id, .. },
            ..
        } = event
        {
            if event_id == request_id {
                seen.insert(i);
            }
        }
    }
}

struct ConfirmedView {
    session_id: SignSessionId,
    subset: Vec<ConfirmedSubsetEntry>,
    sign_task: WireSignTask,
    sp: Option<remote_signing::SpSubset>,
}

async fn wait_for_round_confirmed(
    rx: &mut mpsc::Receiver<(usize, ChannelEvent)>,
    n: usize,
    request_id: EventId,
) -> Vec<ConfirmedView> {
    let mut per_side: BTreeMap<usize, ConfirmedView> = BTreeMap::new();
    while per_side.len() < n {
        let (i, event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for RoundConfirmed")
            .expect("event channel closed");
        if let ChannelEvent::Signing {
            event:
                SigningEvent::RoundConfirmed {
                    request_id: rid,
                    subset,
                    session_id,
                    sign_task,
                    sp,
                    ..
                },
            ..
        } = event
        {
            if rid == request_id {
                per_side.entry(i).or_insert(ConfirmedView {
                    session_id,
                    subset,
                    sign_task,
                    sp,
                });
            }
        }
    }
    per_side.into_values().collect()
}

struct PendingView {
    observed: Vec<EventId>,
    threshold: usize,
}

async fn wait_for_round_pending(
    rx: &mut mpsc::Receiver<(usize, ChannelEvent)>,
    n: usize,
    request_id: EventId,
) -> Vec<PendingView> {
    let mut per_side: BTreeMap<usize, PendingView> = BTreeMap::new();
    while per_side.len() < n {
        let (i, event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for RoundPending")
            .expect("event channel closed");
        if let ChannelEvent::Signing {
            event:
                SigningEvent::RoundPending {
                    request_id: rid,
                    observed,
                    threshold,
                    ..
                },
            ..
        } = event
        {
            if rid == request_id {
                per_side.entry(i).or_insert(PendingView {
                    observed,
                    threshold,
                });
            }
        }
    }
    per_side.into_values().collect()
}

async fn wait_for_partials(
    rx: &mut mpsc::Receiver<(usize, ChannelEvent)>,
    n: usize,
    t: usize,
    request_id: EventId,
) {
    let target = n * t;
    let mut count = 0;
    while count < target {
        let (_i, event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for Partials")
            .expect("event channel closed");
        if let ChannelEvent::Signing {
            event: SigningEvent::Partial {
                request_id: rid, ..
            },
            ..
        } = event
        {
            if rid == request_id {
                count += 1;
            }
        }
    }
}

struct SpChannel {
    run: Run,
    env: SigningEnv,
    rng: ChaCha20Rng,
    sides: Vec<NostrSide>,
    request_id: EventId,
    event_rx: mpsc::Receiver<(usize, ChannelEvent)>,
    committed: WireSignTask,
    access_structure_ref: frostsnap_core::AccessStructureRef,
    master_appkey: MasterAppkey,
    key_context: frostsnap_core::coordinator::KeyContext,
    paths: [BitcoinBip32Path; 2],
    recipients: [SilentPaymentRecipient; 2],
    _relay: MockRelay,
}

async fn prepare_sp_channel(seed: u64) -> SpChannel {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let mut env = SigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1, 1],
        2,
        "sp nostr key",
        KeyPurpose::Bitcoin(Network::Bitcoin),
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let (key_context, init_data, _key_data) = extract_key_artifacts(&run);
    let access_structure_ref = extract_ar(&run);
    let master_appkey = run.participants[0]
        .coordinator
        .iter_keys()
        .next()
        .unwrap()
        .complete_key
        .master_appkey;
    let (first, first_address) = recipient(0x11, 0x12);
    let (second, second_address) = recipient(0x14, 0x15);
    let paths = [
        BitcoinBip32Path::external(7),
        BitcoinBip32Path::internal(42),
    ];
    let template = template(
        master_appkey,
        &[(paths[0], 42_000), (paths[1], 1_337_000)],
        &[
            (50_000, first, first_address),
            (70_000, second, second_address),
        ],
        1_000_000,
    );
    let committed = WireSignTask::BitcoinTransaction(template);

    let (relay_url, relay) = start_relay().await;
    let (event_tx, mut event_rx) = mpsc::channel::<(usize, ChannelEvent)>(512);
    let sides = spawn_sides(&run, 3, &relay_url, &key_context, &init_data, event_tx).await;
    settle_subscriptions().await;
    let request_id = sides[0]
        .handle
        .send_sign_request(&sides[0].keys, committed.clone(), "pay silent".into())
        .await
        .unwrap();
    wait_for_request(&mut event_rx, 3, request_id).await;

    SpChannel {
        run,
        env,
        rng,
        sides,
        request_id,
        event_rx,
        committed,
        access_structure_ref,
        master_appkey,
        key_context,
        paths,
        recipients: [first, second],
        _relay: relay,
    }
}

fn offer_sp(fx: &mut SpChannel, i: usize) -> SignOffer {
    let device_id = *fx.run.participants[i].devices.keys().next().unwrap();
    let (ecdh_session, sends) = fx.run.participants[i]
        .coordinator
        .start_remote_sp_ecdh(
            fx.access_structure_ref,
            fx.committed.clone(),
            device_id,
            TEST_ENCRYPTION_KEY,
            &mut fx.rng,
        )
        .unwrap();
    fx.run.extend_from_coordinator(i, sends);
    fx.run.run_until_finished(&mut fx.env, &mut fx.rng).unwrap();
    fx.run.participants[i]
        .coordinator
        .offer_to_sign_sp(
            RemoteSignSessionId::new([3u8; 32]),
            fx.access_structure_ref,
            fx.committed.clone(),
            device_id,
            ecdh_session,
        )
        .unwrap()
}

async fn finish_sp_round(fx: &mut SpChannel, confirmed: &ConfirmedView, offer_ids: &[EventId]) {
    let sp = confirmed
        .sp
        .clone()
        .expect("silent payment round completes the template");
    let subset_ids: BTreeSet<_> = confirmed
        .subset
        .iter()
        .map(|entry| entry.event_id)
        .collect();
    assert!(subset_ids.is_subset(&offer_ids.iter().copied().collect()));
    assert!(bitcoin_template(&fx.committed)
        .silent_payment_outputs()
        .all(|output| output.script.is_none()));

    let completed = bitcoin_template(&sp.completed_task).clone();
    assert!(completed
        .silent_payment_outputs()
        .all(|output| output.script.is_some()));
    let inputs: Vec<_> = fx
        .paths
        .iter()
        .copied()
        .zip(completed.inputs().iter().map(|input| input.outpoint()))
        .collect();
    let expected = expected_output_keys(
        reconstruct_root_secret(&fx.run, &[0, 1]),
        &inputs,
        &fx.recipients,
    );
    assert_eq!(sp_output_keys(&completed), expected);

    let binonces: Vec<_> = confirmed
        .subset
        .iter()
        .flat_map(|entry| entry.binonces.iter().cloned())
        .collect();
    let authors: BTreeSet<_> = confirmed.subset.iter().map(|entry| entry.author).collect();
    for i in 0..fx.sides.len() {
        let author = frostsnap_nostr::PublicKey::from(fx.sides[i].keys.public_key());
        if !authors.contains(&author) {
            continue;
        }
        let device_id = *fx.run.participants[i].devices.keys().next().unwrap();
        let sign_req = fx.run.participants[i]
            .coordinator
            .sign_with_nonce_reservation(
                RemoteSignSessionId::new([3u8; 32]),
                device_id,
                &binonces,
                Some(&sp),
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        fx.run.extend_from_coordinator(i, sign_req);
    }
    fx.run.run_until_finished(&mut fx.env, &mut fx.rng).unwrap();

    let offer_subset: Vec<EventId> = confirmed
        .subset
        .iter()
        .map(|entry| entry.event_id)
        .collect();
    let mut all_shares = Vec::new();
    for i in 0..fx.sides.len() {
        let author = frostsnap_nostr::PublicKey::from(fx.sides[i].keys.public_key());
        if !authors.contains(&author) {
            continue;
        }
        let shares = fx.env.shares[&(i, confirmed.session_id)][0].clone();
        fx.sides[i]
            .handle
            .send_sign_partial(
                &fx.sides[i].keys,
                fx.request_id,
                offer_subset.clone(),
                shares.clone(),
            )
            .await
            .unwrap();
        all_shares.push(shares);
    }
    wait_for_partials(&mut fx.event_rx, 3, 2, fx.request_id).await;

    let refs: Vec<_> = all_shares.iter().collect();
    let signatures = remote_signing::combine_signatures(
        sp.completed_task.clone(),
        &fx.key_context,
        &binonces,
        &refs,
    )
    .unwrap();
    let checked = sp
        .completed_task
        .clone()
        .check(fx.master_appkey, KeyPurpose::Bitcoin(Network::Bitcoin))
        .unwrap();
    let decoded: Vec<_> = signatures
        .iter()
        .map(|sig| (*sig).into_decoded().unwrap())
        .collect();
    assert!(checked.verify_final_signatures(&Schnorr::<sha2::Sha256>::verify_only(), &decoded));

    let mut tx = completed.to_rust_bitcoin_tx();
    for (input, signature) in tx.input.iter_mut().zip(&decoded) {
        input.witness = Witness::from_slice(&[signature.to_bytes()]);
    }
    for key in &expected {
        assert!(
            tx.output.iter().any(|output| {
                output.script_pubkey.is_p2tr() && output.script_pubkey.as_bytes()[2..34] == key[..]
            }),
            "the final transaction pays {}",
            frostsnap_core::hex::encode(key)
        );
    }
}

async fn wait_for_round_confirmed_and_rejections(
    rx: &mut mpsc::Receiver<(usize, ChannelEvent)>,
    n: usize,
    request_id: EventId,
) -> (Vec<ConfirmedView>, BTreeSet<EventId>) {
    let mut per_side: BTreeMap<usize, ConfirmedView> = BTreeMap::new();
    let mut rejected = BTreeSet::new();
    while per_side.len() < n {
        let (i, event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout waiting for RoundConfirmed")
            .expect("event channel closed");
        match event {
            ChannelEvent::Signing {
                event: SigningEvent::Rejected { event_id, .. },
                ..
            } => {
                rejected.insert(event_id);
            }
            ChannelEvent::Signing {
                event:
                    SigningEvent::RoundConfirmed {
                        request_id: rid,
                        subset,
                        session_id,
                        sign_task,
                        sp,
                        ..
                    },
                ..
            } if rid == request_id => {
                per_side.entry(i).or_insert(ConfirmedView {
                    session_id,
                    subset,
                    sign_task,
                    sp,
                });
            }
            _ => {}
        }
    }
    (per_side.into_values().collect(), rejected)
}

fn sp_output_keys(template: &TransactionTemplate) -> Vec<[u8; 32]> {
    template
        .silent_payment_outputs()
        .map(|output| {
            output.script.as_ref().unwrap().as_bytes()[2..34]
                .try_into()
                .unwrap()
        })
        .collect()
}
