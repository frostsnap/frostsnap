use frostsnap_core::test::*;
use proptest::{
    prelude::*,
    sample,
    test_runner::{Config, RngAlgorithm, TestRng},
};
use std::collections::{BTreeMap, BTreeSet};

use frostsnap_core::{
    coordinator::{BeginKeygen, CoordinatorToUserMessage},
    device::{DeviceToUserMessage, KeyGenPhase3, KeyPurpose},
    DeviceId, KeygenId,
};
use proptest_state_machine::{
    prop_state_machine, strategy::ReferenceStateMachine, StateMachineTest,
};

// ============================================================================
// Reference State
// ============================================================================

#[derive(Clone, Debug)]
struct RefState {
    run: Run,
    waiting_for_acks: Vec<RefKeygen>,
    waiting_for_confirm: Vec<RefKeygen>,
    finished_keygens: Vec<RefFinishedKey>,
}

#[derive(Clone, Debug)]
struct RefKeygen {
    devices: Vec<DeviceId>,
    threshold: u16,
    devices_acked: BTreeSet<DeviceId>,
}

#[derive(Clone, Debug)]
struct RefFinishedKey {
    #[allow(dead_code)]
    devices: Vec<DeviceId>,
    #[allow(dead_code)]
    threshold: u16,
}

// ============================================================================
// Transitions
// ============================================================================

#[derive(Clone, Debug)]
enum Transition {
    StartKeygen {
        devices: Vec<DeviceId>,
        threshold: u16,
    },
    DKeygenAck {
        keygen_index: usize,
        device_id: DeviceId,
    },
    CKeygenConfirm {
        keygen_index: usize,
    },
}

// ============================================================================
// Reference State Machine
// ============================================================================

impl ReferenceStateMachine for RefState {
    type State = RefState;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        let n_coordinators = 2..5usize;
        n_coordinators
            .prop_flat_map(|n| proptest::collection::vec(1..4usize, n))
            .prop_map(|device_counts| {
                let mut rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
                let run = Run::generate_remote(&device_counts, &mut rng);
                RefState {
                    run,
                    waiting_for_acks: vec![],
                    waiting_for_confirm: vec![],
                    finished_keygens: vec![],
                }
            })
            .boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let mut trans = vec![];

        // StartKeygen
        {
            let all_devices = state.run.all_device_ids();
            if !all_devices.is_empty() {
                let possible_devices = sample::select(all_devices);
                let devices_and_threshold = proptest::collection::vec(
                    possible_devices,
                    1..=state.run.all_device_ids().len(),
                )
                .prop_map(|devs| {
                    let deduped: Vec<_> = devs
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    deduped
                })
                .prop_filter("need at least one device", |d| !d.is_empty())
                .prop_flat_map(|devices| {
                    let len = devices.len();
                    (Just(devices), 1..=len)
                });

                let keygen_trans = devices_and_threshold
                    .prop_map(|(devices, threshold)| Transition::StartKeygen {
                        devices,
                        threshold: threshold as u16,
                    })
                    .boxed();

                trans.push((2, keygen_trans));
            }
        }

        // DKeygenAck
        for (keygen_index, keygen) in state.waiting_for_acks.iter().enumerate() {
            let candidates: Vec<_> = keygen
                .devices
                .iter()
                .filter(|d| !keygen.devices_acked.contains(d))
                .copied()
                .collect();
            if !candidates.is_empty() {
                let ack_trans = sample::select(candidates)
                    .prop_map(move |device_id| Transition::DKeygenAck {
                        keygen_index,
                        device_id,
                    })
                    .boxed();
                trans.push((10, ack_trans));
            }
        }

        // CKeygenConfirm
        if !state.waiting_for_confirm.is_empty() {
            let confirm_trans =
                sample::select((0..state.waiting_for_confirm.len()).collect::<Vec<_>>())
                    .prop_map(|keygen_index| Transition::CKeygenConfirm { keygen_index })
                    .boxed();
            trans.push((10, confirm_trans));
        }

        proptest::strategy::Union::new_weighted(trans).boxed()
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::StartKeygen { devices, threshold } => {
                let all = state.run.all_device_ids();
                !devices.is_empty()
                    && *threshold as usize <= devices.len()
                    && devices.iter().all(|d| all.contains(d))
            }
            Transition::DKeygenAck {
                keygen_index,
                device_id,
            } => match state.waiting_for_acks.get(*keygen_index) {
                Some(keygen) => {
                    keygen.devices.contains(device_id) && !keygen.devices_acked.contains(device_id)
                }
                None => false,
            },
            Transition::CKeygenConfirm { keygen_index } => {
                *keygen_index < state.waiting_for_confirm.len()
            }
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::StartKeygen { devices, threshold } => {
                state.waiting_for_acks.push(RefKeygen {
                    devices: devices.clone(),
                    threshold: *threshold,
                    devices_acked: BTreeSet::new(),
                });
            }
            Transition::DKeygenAck {
                keygen_index,
                device_id,
            } => {
                let keygen = &mut state.waiting_for_acks[*keygen_index];
                keygen.devices_acked.insert(*device_id);
                if keygen.devices_acked.len() == keygen.devices.len() {
                    let keygen = state.waiting_for_acks.remove(*keygen_index);
                    state.waiting_for_confirm.push(keygen);
                }
            }
            Transition::CKeygenConfirm { keygen_index } => {
                let keygen = state.waiting_for_confirm.remove(*keygen_index);
                state.finished_keygens.push(RefFinishedKey {
                    devices: keygen.devices,
                    threshold: keygen.threshold,
                });
            }
        }
        state
    }
}

// ============================================================================
// System Under Test
// ============================================================================

struct RemoteKeygenTest {
    run: Run,
    rng: TestRng,
    env: RemoteKeygenEnv,
    waiting_for_acks_ids: Vec<KeygenId>,
    waiting_for_confirm_ids: Vec<KeygenId>,
    finished_key_appkeys: Vec<Vec<frostsnap_core::MasterAppkey>>,
}

#[derive(Default, Debug)]
struct RemoteKeygenEnv {
    device_keygen_phases: BTreeMap<KeygenId, BTreeMap<DeviceId, KeyGenPhase3>>,
}

impl Env for RemoteKeygenEnv {
    fn user_react_to_coordinator(
        &mut self,
        _run: &mut Run,
        _coordinator_index: usize,
        _message: CoordinatorToUserMessage,
        _rng: &mut impl rand_chacha::rand_core::RngCore,
    ) {
        // Don't auto-finalize — FinalizeKeygen transition handles it
    }

    fn user_react_to_device(
        &mut self,
        _run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        _rng: &mut impl rand_chacha::rand_core::RngCore,
    ) {
        if let DeviceToUserMessage::CheckKeyGen { phase, .. } = message {
            self.device_keygen_phases
                .entry(phase.keygen_id)
                .or_default()
                .insert(from, *phase);
        }
    }
}

impl StateMachineTest for RemoteKeygenTest {
    type SystemUnderTest = Self;
    type Reference = RefState;

    fn init_test(
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        let rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
        RemoteKeygenTest {
            run: ref_state.run.clone(),
            rng,
            env: RemoteKeygenEnv::default(),
            waiting_for_acks_ids: vec![],
            waiting_for_confirm_ids: vec![],
            finished_key_appkeys: vec![],
        }
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        let RemoteKeygenTest {
            run,
            rng,
            env,
            waiting_for_acks_ids,
            waiting_for_confirm_ids,
            finished_key_appkeys,
        } = &mut state;

        match transition {
            Transition::StartKeygen { devices, threshold } => {
                let begin = BeginKeygen::new(
                    devices,
                    threshold,
                    "proptest-key".into(),
                    KeyPurpose::Test,
                    rng,
                );
                let keygen_id = begin.keygen_id;
                run.start_remote_keygen(begin, rng);
                run.run_until_finished(env, rng).unwrap();
                waiting_for_acks_ids.push(keygen_id);
            }
            Transition::DKeygenAck {
                keygen_index,
                device_id,
            } => {
                let keygen_id = waiting_for_acks_ids[keygen_index];
                let phase = env
                    .device_keygen_phases
                    .get_mut(&keygen_id)
                    .expect("keygen phases should exist")
                    .remove(&device_id)
                    .expect("device phase should exist");
                let ci = run.owner_of(device_id);
                let ack = run.participants[ci]
                    .devices
                    .get_mut(&device_id)
                    .unwrap()
                    .keygen_ack(phase, &mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(device_id, ack);
                run.run_until_finished(env, rng).unwrap();

                let phases_remaining = env
                    .device_keygen_phases
                    .get(&keygen_id)
                    .map(|m| m.len())
                    .unwrap_or(0);
                if phases_remaining == 0 {
                    let id = waiting_for_acks_ids.remove(keygen_index);
                    waiting_for_confirm_ids.push(id);
                }
            }
            Transition::CKeygenConfirm { keygen_index } => {
                let keygen_id = waiting_for_confirm_ids.remove(keygen_index);

                // Session hash agreement check
                if let Some(phases) = env.device_keygen_phases.get(&keygen_id) {
                    let hashes: BTreeSet<_> = phases.values().map(|p| p.session_hash()).collect();
                    assert!(
                        hashes.len() <= 1,
                        "session hash mismatch across devices in keygen {keygen_id:?}: {hashes:?}"
                    );
                }

                // Finalize on each coordinator (user verified session hash out-of-band)
                for ci in 0..run.participants.len() {
                    let sends = run.participants[ci]
                        .coordinator
                        .finalize_remote_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                        .unwrap();
                    run.extend_from_coordinator(ci, sends);
                }
                run.run_until_finished(env, rng).unwrap();

                let n_keys_before = finished_key_appkeys.len();
                let appkeys: Vec<_> = run
                    .participants
                    .iter()
                    .filter_map(|p| {
                        p.coordinator
                            .iter_keys()
                            .nth(n_keys_before)
                            .map(|k| k.complete_key.master_appkey)
                    })
                    .collect();
                finished_key_appkeys.push(appkeys);
            }
        }

        state
    }

    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        assert_eq!(
            state.finished_key_appkeys.len(),
            ref_state.finished_keygens.len(),
            "finished keygen count mismatch"
        );

        for (i, appkeys) in state.finished_key_appkeys.iter().enumerate() {
            assert!(
                !appkeys.is_empty(),
                "keygen {i} should have produced keys on at least one coordinator"
            );
            let first = &appkeys[0];
            for (ci, appkey) in appkeys.iter().enumerate() {
                assert_eq!(
                    first, appkey,
                    "keygen {i}: coordinator {ci} disagrees on master_appkey"
                );
            }
        }

        for (ci, p) in state.run.participants.iter().enumerate() {
            assert_eq!(
                p.coordinator.iter_keys().count(),
                ref_state.finished_keygens.len(),
                "coordinator {ci} has wrong number of keys"
            );
        }
    }
}

prop_state_machine! {
    #![proptest_config(Config {
        verbose: 1,
        cases: 256,
        .. Config::default()
    })]

    #[test]
    fn remote_keygen_state_machine(
        sequential
        5..15
        =>
        RemoteKeygenTest
    );
}
