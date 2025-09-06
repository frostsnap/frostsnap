mod common;
use common::*;
use proptest::{
    array,
    prelude::*,
    sample,
    test_runner::{Config, RngAlgorithm, TestRng},
};
use std::collections::{BTreeMap, BTreeSet};

use frostsnap_core::{
    coordinator::{
        CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, CoordinatorToUserSigningMessage,
    },
    device::{DeviceToUserMessage, KeyGenPhase2, KeyPurpose, SignPhase1},
    message::keygen,
    AccessStructureRef, DeviceId, KeygenId, SignSessionId, WireSignTask,
};
use proptest_state_machine::{
    prop_state_machine, strategy::ReferenceStateMachine, StateMachineTest,
};

#[derive(Clone, Debug)]
struct RefState {
    run_start: Run,
    pending_keygens: BTreeMap<KeygenId, RefKeygen>,
    finished_keygens: Vec<RefFinishedKey>,
    sign_sessions: Vec<RefSignSession>,
    got_nonces_from: BTreeSet<DeviceId>,
    n_nonce_slots: usize,
    n_desired_nonce_streams_coord: usize,
}

impl RefState {
    pub fn n_devices(&self) -> usize {
        self.run_start.devices.len()
    }

    pub fn available_signing_devices(&self) -> BTreeSet<DeviceId> {
        let mut device_counter = self
            .run_start
            .device_vec()
            .into_iter()
            .filter(|id| self.got_nonces_from.contains(id))
            .map(|id| {
                (
                    id,
                    self.n_nonce_slots.min(self.n_desired_nonce_streams_coord),
                )
            })
            .collect::<BTreeMap<_, _>>();

        for session in &self.sign_sessions {
            for device in &session.devices {
                *device_counter.get_mut(device).unwrap() -= 1;
            }
        }

        device_counter
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(id, _)| id)
            .collect()
    }
}

#[derive(Clone, Debug)]
struct RefSignSession {
    key_index: usize,
    devices: BTreeSet<DeviceId>,
    #[allow(unused)]
    message: String,
    got_sigs_from: BTreeSet<DeviceId>,
    sent_req_to: BTreeSet<DeviceId>,
    canceled: bool,
}

impl RefSignSession {
    pub fn finished(&self) -> bool {
        self.devices == self.got_sigs_from
    }
}

#[derive(Clone, Debug)]
struct RefKeygen {
    do_keygen: keygen::Begin,
    devices_confirmed: BTreeSet<DeviceId>,
}

#[derive(Clone, Debug)]
struct RefFinishedKey {
    do_keygen: keygen::Begin,
    deleted: bool,
}

#[derive(Clone, Debug)]
enum Transition {
    CStartKeygen(keygen::Begin),
    DKeygenAck {
        device_id: DeviceId,
        keygen_id: KeygenId,
    },
    CKeygenConfirm {
        keygen_id: KeygenId,
    },
    CNonceReplenish {
        device_id: DeviceId,
    },
    CStartSign {
        key_index: usize,
        devices: BTreeSet<DeviceId>,
        message: String,
    },
    CSendSignRequest {
        session_index: usize,
        device_id: DeviceId,
    },
    CCancelSignSession {
        session_index: usize,
    },
    DAckSignRequest {
        session_index: usize,
        device_id: DeviceId,
    },
    CDeleteKey {
        key_index: usize,
    },
}

impl ReferenceStateMachine for RefState {
    type State = RefState;

    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        (1u16..10, 1usize..8, 1usize..8)
            .prop_map(
                move |(n_devices, n_nonce_slots, n_desired_nonce_streams_coord)| {
                    let mut rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
                    let run =
                        Run::generate_with_nonce_slots(n_devices.into(), &mut rng, n_nonce_slots);

                    RefState {
                        run_start: run,
                        pending_keygens: Default::default(),
                        finished_keygens: Default::default(),
                        sign_sessions: Default::default(),
                        got_nonces_from: Default::default(),
                        n_nonce_slots,
                        n_desired_nonce_streams_coord,
                    }
                },
            )
            .boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let mut trans = vec![];

        {
            let possible_devices = sample::select(state.run_start.device_vec());
            let devices_and_threshold =
                proptest::collection::btree_set(possible_devices, 1..=state.n_devices())
                    .prop_flat_map(|devices| (Just(devices.clone()), 1..=devices.len()));
            let name = proptest::string::string_regex("[A-Z][a-z][a-z]")
                .unwrap()
                .no_shrink();
            let keygen_id = array::uniform::<_, 16>(0..=u8::MAX)/* testing colliding keygen ids is not of interest */ .no_shrink();

            let keygen_trans = (keygen_id, devices_and_threshold, name)
                .prop_map(|(keygen_id, (devices, threshold), key_name)| {
                    Transition::CStartKeygen(keygen::Begin::new_with_id(
                        devices.into_iter().collect(),
                        threshold as u16,
                        key_name,
                        KeyPurpose::Test,
                        KeygenId::from_bytes(keygen_id),
                    ))
                })
                .boxed();

            trans.push((1, keygen_trans));
        }

        for (&keygen_id, keygen) in &state.pending_keygens {
            let candidates = keygen
                .do_keygen
                .devices
                .iter()
                .filter(|device_id| !keygen.devices_confirmed.contains(device_id))
                .cloned()
                .collect::<Vec<_>>();

            if candidates.is_empty() {
                trans.push((10, Just(Transition::CKeygenConfirm { keygen_id }).boxed()));
            } else {
                let device_ack = sample::select(candidates)
                    .prop_map(move |device_id| Transition::DKeygenAck {
                        keygen_id,
                        device_id,
                    })
                    .boxed();

                trans.push((10, device_ack));
            }
        }

        let nonce_req = sample::select(state.run_start.device_vec())
            .prop_map(|device_id| Transition::CNonceReplenish { device_id })
            .boxed();

        trans.push((3, nonce_req));

        // sign request
        {
            let candidate_keys = state
                .finished_keygens
                .iter()
                .cloned()
                .enumerate()
                .filter_map(|(key_index, key)| {
                    let available = key
                        .do_keygen
                        .device_set()
                        .intersection(&state.available_signing_devices())
                        .cloned()
                        .collect::<Vec<_>>();

                    if available.len() > key.do_keygen.threshold as usize {
                        Some((key_index, key, available))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if !candidate_keys.is_empty() {
                let start_sign = sample::select(candidate_keys)
                    .prop_flat_map(|(key_index, key, available_devices)| {
                        let sample = sample::select(available_devices);
                        let signing_set = proptest::collection::btree_set(
                            sample,
                            key.do_keygen.threshold as usize,
                        );

                        let message = proptest::string::string_regex("[a-z][a-z][a-z]").unwrap();

                        (signing_set, message).prop_map(move |(devices, message)| {
                            Transition::CStartSign {
                                key_index,
                                devices,
                                message,
                            }
                        })
                    })
                    .boxed();

                trans.push((2, start_sign));
            }
        }

        for (index, session) in state.sign_sessions.iter().enumerate() {
            // coord send sign request
            {
                let candidates = session
                    .devices
                    .difference(&session.got_sigs_from)
                    .cloned()
                    .collect::<Vec<_>>();
                if candidates.is_empty() {
                    // TODO
                } else {
                    let next_to_ask = sample::select(candidates);
                    let sign_req = next_to_ask
                        .prop_map(move |device_id| Transition::CSendSignRequest {
                            session_index: index,
                            device_id,
                        })
                        .boxed();
                    trans.push((10, sign_req));
                }
            }

            // device ack sign request
            {
                let candidates = session
                    .sent_req_to
                    .difference(&session.got_sigs_from)
                    .copied()
                    .collect::<Vec<_>>();
                if !candidates.is_empty() {
                    let selected = sample::select(candidates);
                    let ack_sign = selected
                        .prop_map(move |device_id| Transition::DAckSignRequest {
                            session_index: index,
                            device_id,
                        })
                        .boxed();
                    trans.push((10, ack_sign));
                }
            }
        }

        // Coordinator cancel
        if !state.sign_sessions.is_empty() {
            let cancel_session = sample::select((0..state.sign_sessions.len()).collect::<Vec<_>>())
                .prop_map(|session_index| Transition::CCancelSignSession { session_index })
                .boxed();

            trans.push((1, cancel_session));
        }

        if !state.finished_keygens.is_empty() {
            let deletion_candidate =
                sample::select((0..state.finished_keygens.len()).collect::<Vec<_>>());
            let to_delete = deletion_candidate
                .prop_map(|key_index| Transition::CDeleteKey { key_index })
                .boxed();
            trans.push((1, to_delete));
        }

        proptest::strategy::Union::new_weighted(trans).boxed()
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::CStartKeygen(do_key_gen) => {
                !state.pending_keygens.contains_key(&do_key_gen.keygen_id)
                    && state
                        .run_start
                        .device_set()
                        .is_superset(&do_key_gen.device_set())
            }
            Transition::DKeygenAck {
                device_id,
                keygen_id,
            } => match state.pending_keygens.get(keygen_id) {
                Some(keygen_state) => {
                    keygen_state.do_keygen.devices.contains(device_id)
                        && !keygen_state.devices_confirmed.contains(device_id)
                }
                None => false,
            },
            Transition::CKeygenConfirm { keygen_id } => {
                match state.pending_keygens.get(keygen_id) {
                    Some(keygen_state) => {
                        keygen_state.devices_confirmed.len() == keygen_state.do_keygen.devices.len()
                    }
                    None => false,
                }
            }
            Transition::CNonceReplenish { device_id } => {
                state.run_start.device_set().contains(device_id)
            }
            Transition::CStartSign {
                key_index, devices, ..
            } => match state.finished_keygens.get(*key_index) {
                Some(keygen) => {
                    !keygen.deleted
                        && keygen.do_keygen.device_set().is_superset(devices)
                        && state.available_signing_devices().is_superset(devices)
                }
                None => false,
            },
            Transition::CSendSignRequest {
                session_index,
                device_id,
            } => match state.sign_sessions.get(*session_index) {
                Some(session) => {
                    session.devices.contains(device_id)
                        && session.key_index < state.finished_keygens.len()
                        && !state.finished_keygens[session.key_index].deleted
                        && !session.canceled
                }
                None => false,
            },
            Transition::CCancelSignSession { session_index } => {
                match state.sign_sessions.get(*session_index) {
                    Some(session) => {
                        session.key_index < state.finished_keygens.len()
                            && !state.finished_keygens[session.key_index].deleted
                            && !session.canceled
                    }
                    None => false,
                }
            }
            Transition::DAckSignRequest {
                session_index,
                device_id,
            } => match state.sign_sessions.get(*session_index) {
                Some(session) => {
                    session.sent_req_to.contains(device_id)
                        && session.key_index < state.finished_keygens.len()
                        && !state.finished_keygens[session.key_index].deleted
                        && !session.got_sigs_from.contains(device_id)
                        && !session.canceled
                }
                None => false,
            },
            &Transition::CDeleteKey { key_index } => key_index < state.finished_keygens.len(),
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition.clone() {
            Transition::CStartKeygen(do_keygen) => {
                state.pending_keygens.insert(
                    do_keygen.keygen_id,
                    RefKeygen {
                        do_keygen,
                        devices_confirmed: Default::default(),
                    },
                );
            }
            Transition::DKeygenAck {
                keygen_id,
                device_id,
            } => {
                if let Some(state) = state.pending_keygens.get_mut(&keygen_id) {
                    state.devices_confirmed.insert(device_id);
                }
            }
            Transition::CKeygenConfirm { keygen_id } => {
                if let Some(keygen) = state.pending_keygens.remove(&keygen_id) {
                    state.finished_keygens.push(RefFinishedKey {
                        do_keygen: keygen.do_keygen,
                        deleted: false,
                    });
                }
            }
            Transition::CNonceReplenish { device_id } => {
                state.got_nonces_from.insert(device_id);
            }
            Transition::CStartSign {
                key_index,
                devices,
                message,
            } => state.sign_sessions.push(RefSignSession {
                key_index,
                devices,
                message,
                got_sigs_from: Default::default(),
                sent_req_to: Default::default(),
                canceled: false,
            }),
            Transition::CSendSignRequest {
                session_index,
                device_id,
            } => {
                state
                    .sign_sessions
                    .get_mut(session_index)
                    .unwrap()
                    .sent_req_to
                    .insert(device_id);
            }
            Transition::CCancelSignSession { session_index } => {
                state.sign_sessions.get_mut(session_index).unwrap().canceled = true;
            }
            Transition::DAckSignRequest {
                session_index,
                device_id,
            } => {
                let session = &mut state.sign_sessions[session_index];
                session.got_sigs_from.insert(device_id);
            }
            Transition::CDeleteKey { key_index } => {
                for session in &mut state.sign_sessions {
                    if session.key_index == key_index {
                        session.canceled = true;
                    }
                }
                state.finished_keygens[key_index].deleted = true;
            }
        }

        state
    }
}

/// This tests that all valid transitions can occur without panicking. This has marginal benefit for
/// security but tests any state transition the user should be able to make happen while using the system.
struct HappyPathTest {
    run: Run,
    rng: TestRng,
    env: ProptestEnv,
    finished_keygens: Vec<AccessStructureRef>,
    sign_sessions: Vec<SignSessionId>,
}

#[derive(Default, Debug)]
pub struct ProptestEnv {
    device_keygen_acks: BTreeMap<KeygenId, BTreeMap<DeviceId, KeyGenPhase2>>,
    sign_reqs: BTreeMap<SignSessionId, BTreeMap<DeviceId, SignPhase1>>,
    coordinator_keygen_acks: BTreeSet<KeygenId>,
    finished_signatures: BTreeSet<SignSessionId>,
}

impl Env for ProptestEnv {
    fn user_react_to_coordinator(
        &mut self,
        _run: &mut Run,
        message: CoordinatorToUserMessage,
        _rng: &mut impl RngCore,
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
                self.coordinator_keygen_acks.insert(keygen_id);
            }
            CoordinatorToUserMessage::Signing(signing_update) => match signing_update {
                CoordinatorToUserSigningMessage::GotShare { .. } => { /* ignore for now */ }
                CoordinatorToUserSigningMessage::Signed { session_id, .. } => {
                    self.finished_signatures.insert(session_id);
                }
            },
            _ => { /* nothing needs doing */ }
        }
    }

    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        _rng: &mut impl RngCore,
    ) {
        use DeviceToUserMessage::*;
        match message {
            FinalizeKeyGen { .. } => {
                // TODO: Do we need to keep track of keygen-finalized messages received by the user?
                // TODO: Ignore for now.
            }
            CheckKeyGen { phase, .. } => {
                let pending = self.device_keygen_acks.entry(phase.keygen_id).or_default();
                pending.insert(from, *phase);
            }
            SignatureRequest { phase } => {
                self.sign_reqs
                    .entry(phase.session_id)
                    .or_default()
                    .insert(from, *phase);
            }
            Restoration(msg) => {
                use frostsnap_core::device::restoration::ToUserRestoration::*;
                match msg {
                    DisplayBackupRequest { phase } => {
                        let backup_ack = run
                            .device(from)
                            .display_backup_ack(*phase, &mut TestDeviceKeyGen)
                            .unwrap();
                        run.extend_from_device(from, backup_ack);
                    }
                    _ => { /* ignore */ }
                }
            }
            VerifyAddress { .. } => {
                // we dont actually confirm on the device
            }
        }
    }
}

impl StateMachineTest for HappyPathTest {
    type SystemUnderTest = Self;

    type Reference = RefState;

    fn init_test(
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        let rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
        HappyPathTest {
            run: ref_state.run_start.clone(),
            rng,
            env: ProptestEnv::default(),
            finished_keygens: Default::default(),
            sign_sessions: Default::default(),
        }
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        let HappyPathTest {
            run,
            rng,
            env,
            finished_keygens,
            sign_sessions,
        } = &mut state;
        match transition {
            Transition::CStartKeygen(do_keygen) => {
                let do_keygen = run.coordinator.begin_keygen(do_keygen, rng).unwrap();
                run.extend(do_keygen);
            }
            Transition::DKeygenAck {
                keygen_id,
                device_id,
            } => {
                let pending = env.device_keygen_acks.get_mut(&keygen_id).unwrap();
                let phase = pending.remove(&device_id).unwrap();
                let ack = run
                    .device(device_id)
                    .keygen_ack(phase, &mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(device_id, ack);
            }
            Transition::CKeygenConfirm { keygen_id } => {
                if env.coordinator_keygen_acks.remove(&keygen_id) {
                    let send_finalize_keygen = run
                        .coordinator
                        .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                        .unwrap();
                    let access_structure_ref = send_finalize_keygen.access_structure_ref;
                    run.extend(send_finalize_keygen);
                    finished_keygens.push(access_structure_ref);
                } else {
                    panic!("CKeygenConfirm for non-existent keygen");
                }
            }
            Transition::CNonceReplenish { device_id } => {
                let messages = run.coordinator.maybe_request_nonce_replenishment(
                    device_id,
                    ref_state.n_desired_nonce_streams_coord,
                    rng,
                );
                run.extend(messages);
            }
            Transition::CStartSign {
                key_index,
                devices,
                message,
            } => {
                let as_ref = finished_keygens[key_index];
                let session_id = run
                    .coordinator
                    .start_sign(as_ref, WireSignTask::Test { message }, &devices, rng)
                    .unwrap();
                sign_sessions.push(session_id);
            }
            Transition::CSendSignRequest {
                session_index,
                device_id,
            } => {
                let session_id = sign_sessions[session_index];

                let req =
                    run.coordinator
                        .request_device_sign(session_id, device_id, TEST_ENCRYPTION_KEY);
                run.extend(req);
            }
            Transition::CCancelSignSession { session_index } => {
                let session_id = sign_sessions[session_index];
                run.coordinator.cancel_sign_session(session_id);
            }
            Transition::DAckSignRequest {
                session_index,
                device_id,
            } => {
                let session_id = sign_sessions[session_index];

                let phase = env
                    .sign_reqs
                    .get_mut(&session_id)
                    .unwrap()
                    .remove(&device_id)
                    .unwrap();
                let sign_ack = run
                    .device(device_id)
                    .sign_ack(phase, &mut TestDeviceKeyGen)
                    .unwrap();
                run.extend_from_device(device_id, sign_ack);
            }
            Transition::CDeleteKey { key_index } => {
                let as_ref = finished_keygens[key_index];
                run.coordinator.delete_key(as_ref.key_id);
            }
        }

        run.run_until_finished(env, rng).unwrap();
        state
    }

    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        for (session_index, session) in ref_state.sign_sessions.iter().enumerate() {
            if session.finished() {
                let ssid = state.sign_sessions[session_index];
                assert!(state.env.finished_signatures.contains(&ssid));
            }
        }
    }
}

// Setup the state machine test using the `prop_state_machine!` macro
prop_state_machine! {
    #![proptest_config(Config {
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        cases: 512,
        .. Config::default()
    })]

    // NOTE: The `#[test]` attribute is commented out in here so we can run it
    // as an example from the `fn main`.

    #[test]
    fn state_machine_happy(
        // This is a macro's keyword - only `sequential` is currently supported.
        sequential
        // The number of transitions to be generated for each case. This can
        // be a single numerical value or a range as in here.
        30
        // Macro's boilerplate to separate the following identifier.
        =>
        // The name of the type that implements `StateMachineTest`.
        HappyPathTest
    );
}
