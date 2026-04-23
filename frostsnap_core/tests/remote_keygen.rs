use frostsnap_core::test::{Env, Run, Send};
use frostsnap_core::{
    coordinator::{BeginKeygen, BroadcastPayload},
    device::KeyPurpose,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

struct DefaultEnv;
impl Env for DefaultEnv {}

#[test]
fn two_coordinators() {
    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let mut run = Run::generate_remote(&[1, 2], &mut rng);

    let begin = BeginKeygen::new(
        run.all_device_ids(),
        2,
        "two-coord key".into(),
        KeyPurpose::Test,
        &mut rng,
    );
    run.start_remote_keygen(begin, &mut rng);
    run.run_until_finished(&mut DefaultEnv, &mut rng).unwrap();

    let keys: Vec<_> = run
        .participants
        .iter()
        .filter_map(|p| p.coordinator.iter_keys().next())
        .collect();
    assert_eq!(keys.len(), 2, "both coordinators should have the key");
    assert_eq!(
        keys[0].complete_key.master_appkey, keys[1].complete_key.master_appkey,
        "both coordinators produce the same key"
    );
}

#[test]
fn three_coordinators_one_device_each() {
    let mut rng = ChaCha20Rng::seed_from_u64(123);
    let mut run = Run::generate_remote(&[1, 1, 1], &mut rng);

    let begin = BeginKeygen::new(
        run.all_device_ids(),
        2,
        "three-coord key".into(),
        KeyPurpose::Test,
        &mut rng,
    );
    run.start_remote_keygen(begin, &mut rng);
    run.run_until_finished(&mut DefaultEnv, &mut rng).unwrap();

    let keys: Vec<_> = run
        .participants
        .iter()
        .filter_map(|p| p.coordinator.iter_keys().next())
        .collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(
        keys[0].complete_key.master_appkey,
        keys[1].complete_key.master_appkey
    );
    assert_eq!(
        keys[1].complete_key.master_appkey,
        keys[2].complete_key.master_appkey
    );
}

#[test]
fn malicious_coordinator_replaces_device_input() {
    use frostsnap_core::coordinator::remote_keygen::RemoteKeygenPayload;
    use schnorr_fun::frost::ShareIndex;

    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let mut run = Run::generate_remote(&[1, 2], &mut rng);

    let device_ids = run.all_device_ids();
    let target_device = device_ids[0];
    let n_coordinators = run.coordinator_ids().len();

    let begin = BeginKeygen::new(
        device_ids.clone(),
        2,
        "malicious key".into(),
        KeyPurpose::Test,
        &mut rng,
    );
    run.start_remote_keygen(begin, &mut rng);

    let share_receivers_enckeys: std::collections::BTreeMap<_, _> = device_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            (
                ShareIndex::from(core::num::NonZeroU32::new((i + 1) as u32).unwrap()),
                id.pubkey(),
            )
        })
        .collect();

    let input_gen_index =
        device_ids.iter().position(|d| *d == target_device).unwrap() as u32 + n_coordinators as u32;

    let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
    let (_fake_contributor, fake_input) =
        schnorr_fun::frost::chilldkg::certpedpop::Contributor::gen_keygen_input(
            &schnorr,
            2,
            &share_receivers_enckeys,
            input_gen_index,
            &mut rng,
        );

    let owner = run.owner_of(target_device);
    let mut tampered = false;
    let result = run.run_until(&mut DefaultEnv, &mut rng, |run| {
        if !tampered {
            for msg in run.message_queue.iter_mut() {
                if let Send::Broadcast {
                    to,
                    from,
                    payload: BroadcastPayload::RemoteKeygen(ref mut payload),
                    ..
                } = msg
                {
                    if *from == target_device
                        && *to != owner
                        && matches!(payload, RemoteKeygenPayload::Input(_))
                    {
                        *payload = RemoteKeygenPayload::Input(fake_input.clone());
                        tampered = true;
                        break;
                    }
                }
            }
        }
        false
    });

    assert!(tampered, "should have found and tampered a message");
    assert!(
        result.is_err(),
        "protocol must detect the tampered device input"
    );
}

#[test]
fn dropped_input_halts_keygen() {
    use frostsnap_core::coordinator::remote_keygen::RemoteKeygenPayload;

    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let mut run = Run::generate_remote(&[1, 2], &mut rng);
    let device_ids = run.all_device_ids();
    let dropped_device = device_ids[1];

    let begin = BeginKeygen::new(
        device_ids,
        2,
        "drop input".into(),
        KeyPurpose::Test,
        &mut rng,
    );
    run.start_remote_keygen(begin, &mut rng);

    run.run_until(&mut DefaultEnv, &mut rng, |run| {
        run.message_queue.retain(|msg| {
            !matches!(
                msg,
                Send::Broadcast {
                    from,
                    payload: BroadcastPayload::RemoteKeygen(RemoteKeygenPayload::Input(_)),
                    ..
                } if *from == dropped_device
            )
        });
        false
    })
    .unwrap();

    let n_keys: usize = run
        .participants
        .iter()
        .map(|p| p.coordinator.iter_keys().count())
        .sum();
    assert_eq!(n_keys, 0, "keygen should not complete with dropped input");
}

#[test]
fn dropped_certification_halts_keygen() {
    use frostsnap_core::coordinator::remote_keygen::RemoteKeygenPayload;

    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let mut run = Run::generate_remote(&[1, 2], &mut rng);
    let device_ids = run.all_device_ids();
    let dropped_device = device_ids[2];

    let begin = BeginKeygen::new(
        device_ids,
        2,
        "drop cert".into(),
        KeyPurpose::Test,
        &mut rng,
    );
    run.start_remote_keygen(begin, &mut rng);

    run.run_until(&mut DefaultEnv, &mut rng, |run| {
        run.message_queue.retain(|msg| {
            !matches!(
                msg,
                Send::Broadcast {
                    from,
                    payload: BroadcastPayload::RemoteKeygen(RemoteKeygenPayload::Certification(_)),
                    ..
                } if *from == dropped_device
            )
        });
        false
    })
    .unwrap();

    assert!(
        run.participants
            .iter()
            .any(|p| p.coordinator.iter_keys().count() == 0),
        "at least one coordinator should not complete with dropped certification"
    );
}
