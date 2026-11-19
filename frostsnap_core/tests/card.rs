//! The 64-word split card in format C′ end to end: the phone (a `FrostCoordinator`) and one
//! device (a `FrostSigner` with the firmware's `CardSession`) in one process, joined by a USB
//! link that carries one frame at a time. The device persists before it sends, as the firmware
//! does, and the app persists before it sends, as its loop does. So a cut can go anywhere: the
//! device loses power (an unplug), or the app restarts.
//!
//! One scripted user works both screens, as a real one does (spec PK.7, owner B1). It writes
//! words 1–32 off the device's pages and answers the device's quiz, confirms words 1–32 on the
//! phone, writes words 33–64 off the phone's pages and answers its quiz while the device proves,
//! then [Continue], the device round (5 of words 33–64) and the phone round (5 of words 1–32)
//! from what it wrote, picking "None of these" when the word it wrote isn't among the options.
//!
//! On every run `World` collects every card word each screen showed and each frame carried,
//! so the tests check the 37-word bound (design §0, §3.11 #1, #2, #10): the device screen sees
//! words 1–32 and E, the phone words 33–64 and D. It also checks, on every step:
//! - no seal, proof or opening job runs on the device while its record is WRITING, SELF_CHECK
//!   or CARD_CHECKED, and the bit proofs run only in PROVING (spec PK.7);
//! - the phone pulls no proof and shows no word of 33–64 before it persisted the user's confirm
//!   of words 1–32 (`DeviceHalfWritten`), and sends `WriteStart` only once pinned;
//! - N is drawn only at [Continue], with both halves recorded and the seal proven, and never
//!   leaves the phone before the device round's pass is persisted; `CheckStarted` is never
//!   persisted before `DeviceRoundResult{true}`;
//! - `Checked` follows `DeviceHalfWritten`, which follows `Finished`, and the phone's status is
//!   Checked only with all three keys.
mod common;

use common::{Env, Run, TestDeviceKeyGen, TEST_ENCRYPTION_KEY};
use frost_backup::{
    bip39_words::{lookalike_pool, BIP39_WORDS},
    verifiable_card::{
        codec::{self, ImageTable, RepairJob, RepairOutcome, Work},
        layout::{
            self, COEFFS, DEVICE_HALF, FIRST_PHONE_SLOT, FIRST_SHARE_SLOT, HALF, PHONE_HALF,
            THRESHOLD_SLOT, WORDS,
        },
        phone,
        proof::{self, Ops, CHECK_BOXES},
    },
};
use frostsnap_core::coordinator::{
    card::{
        card_setup, BackupFormat, BoxAnswer, CardBackupStatus, CardDeviceWork, CardEvent,
        CardFailReason, CardMutation as CoordCardMutation, CardRestoreEvent, CardToUser,
        DeviceRoundRecord, COMMIT_COUNT, MAX_INTERRUPTION_RESUMES,
    },
    restoration::{PhysicalBackupPhase, ToUserRestoration as CoordToUserRestoration},
    BeginKeygen, CoordinatorSend, CoordinatorToUserMessage, FrostCoordinator,
    Mutation as CoordMutation,
};
use frostsnap_core::device::{
    card::{
        card_gate, CardMutation as DeviceCardMutation, CardOutput, CardRecord, CardScreen,
        CardState, CardUserAction, ShareOrigin, ToUserCard, VerdictReason,
    },
    card_restore::{
        CardEntries, CardRestorePhase, CardRestoreReport, RestoreFailure, RestoreJob,
        RestoreOutcome, RESTORE_DECODE_MS, RESTORE_SEARCH_SLICES, UNREADABLE_THRESHOLD,
    },
    card_session::{BusyTask, CardJobEvent, CardSession},
    restoration::{PhysicalBackup, ThresholdWarning, ToUserRestoration},
    DeviceToUserMessage, FrostSigner, KeyPurpose, Mutation as DeviceMutation,
};
use frostsnap_core::message::{
    card::*, CoordinatorRestoration, CoordinatorToDeviceMessage, DeviceSend,
    DeviceToCoordinatorMessage,
};
use frostsnap_core::{
    AccessStructureRef, CoordShareDecryptionContrib, DeviceId, EnterPhysicalId, Kind, RestorationId,
};
use rand::{rngs::StdRng, CryptoRng, RngCore, SeedableRng};
use schnorr_fun::frost::{ShareImage, ShareIndex, SharedKey};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

struct KeygenEnv;

impl Env for KeygenEnv {}

/// A `t`-of-`n` card keygen, everything persisted on both sides.
fn key(n: usize, t: u16, seed: u64) -> Run {
    let mut run = keygen(n, t, seed);
    persist(&mut run);
    run
}

/// Owner B15: another app install holding the wallet of [`key`]`(n, t, seed)`, which didn't run
/// its keygen: the wallet's own mutations without the keygen's `KeyFormat`, as a restoration
/// leaves it (a restoration never records one).
fn other_install(n: usize, t: u16, seed: u64) -> Phone {
    let mut run = keygen(n, t, seed);
    let mut coordinator = FrostCoordinator::new();
    for mutation in run.coordinator.staged_mutations().clone() {
        if !matches!(
            mutation,
            CoordMutation::Card(CoordCardMutation::KeyFormat { .. })
        ) {
            coordinator.apply_mutation(mutation);
        }
    }
    persist(&mut run);
    let key_id = coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref()
        .key_id;
    assert!(!coordinator.card_created_here(key_id));
    Phone::new(coordinator)
}

/// The keygen of [`key`], with nothing persisted yet.
fn keygen(n: usize, t: u16, seed: u64) -> Run {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut run = Run::generate(n, &mut rng);
    let begin = BeginKeygen::new(
        run.device_vec(),
        t,
        "card key".into(),
        KeyPurpose::Test,
        &mut rng,
    )
    .with_backup_format(BackupFormat::Card64);
    let send = run.coordinator.begin_keygen(begin, &mut rng).unwrap();
    run.extend(send);
    run.run_until_finished(&mut KeygenEnv, &mut rng).unwrap();
    run
}

/// Persists what both sides have staged: the coordinator's log and each device's flash become
/// `start_coordinator` and `start_devices`.
fn persist(run: &mut Run) {
    for mutation in run.coordinator.take_staged_mutations() {
        run.start_coordinator.apply_mutation(mutation);
    }
    for (id, device) in &mut run.devices {
        let flash = run.start_devices.get_mut(id).unwrap();
        for mutation in device.staged_mutations().drain(..) {
            flash.apply_mutation(mutation);
        }
        *flash.nonce_slots() = device.nonce_slots().clone();
    }
}

fn word_index(word: &str) -> u16 {
    BIP39_WORDS.iter().position(|w| *w == word).unwrap() as u16
}

fn encoded<T: bincode::Encode>(value: &T) -> Vec<u8> {
    bincode::encode_to_vec(value, bincode::config::standard()).unwrap()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// One frame on the USB link.
#[derive(Clone, Debug)]
enum Frame {
    ToDevice(CoordinatorCard),
    ToPhone(DeviceCard),
}

impl Frame {
    fn kind(&self) -> &'static str {
        match self {
            Frame::ToDevice(message) => message.kind(),
            Frame::ToPhone(message) => message.kind(),
        }
    }
}

/// What a test does to a frame on the link.
enum Tamper {
    Pass(Frame),
    Drop,
}

type Filter = Box<dyn FnMut(Frame) -> Tamper>;

/// A point where the flow can be cut.
#[derive(Clone, Debug, PartialEq)]
enum Cut {
    /// Before a frame arrives, the device loses power and the frame is lost.
    Device { before: &'static str },
    /// Before a frame arrives, the app restarts and the frame is lost.
    App { before: &'static str },
    /// The device loses power with only `persisted` of one call's staged mutations on flash.
    DeviceFlush {
        persisted: usize,
        staged: Vec<&'static str>,
    },
    /// The app dies before it persists one call's mutations.
    AppPersist { staged: Vec<&'static str> },
}

/// The device's RNG: a seeded one, or (`None`) one that has failed and gives only zeros (T6).
struct DeviceRng(Option<StdRng>);

impl RngCore for DeviceRng {
    fn next_u32(&mut self) -> u32 {
        self.0.as_mut().map_or(0, RngCore::next_u32)
    }
    fn next_u64(&mut self) -> u64 {
        self.0.as_mut().map_or(0, RngCore::next_u64)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        match self.0.as_mut() {
            Some(rng) => rng.fill_bytes(dest),
            None => dest.fill(0),
        }
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for DeviceRng {}

/// What the phone's user does next.
#[derive(Clone, Debug)]
enum PhoneAction {
    /// Opens (or reopens) the card flow: the app's protocol on its first connection, or the
    /// user's Continue after an unplug.
    Begin,
    /// The device is plugged in again while a page of words in RAM kept the flow live: the
    /// app's protocol sends its `Begin` again, without a new flow.
    Replug,
    /// The hold "I've written words 1–32" on the phone (owner B1).
    ConfirmDeviceHalf,
    Page(u8),
    HalfWritten,
    SelfCheck(u16),
    FinalCheck,
    Answer(BoxAnswer),
    /// Owner B9: the answer to "did you cause the interruption?".
    AnswerInterruption(bool),
    /// Owner B15: on an install that didn't run the keygen, the choice to write the card
    /// unverified.
    WriteUnverified,
}

/// One phone's persisted card state, as its log shows it.
#[derive(Default)]
struct PhoneLog {
    /// The card mutations persisted, in order.
    log: Vec<CoordCardMutation>,
    /// N, from when it's persisted as pending.
    pending_n: Option<[u8; 32]>,
    /// `DeviceRoundResult{true}` is persisted.
    round_passed: bool,
    /// N, once `CheckStarted` is persisted.
    n: Option<[u8; 32]>,
    d: Option<[u8; 5]>,
    finished: bool,
    /// The user's confirm of words 1–32 is persisted.
    device_half_written: bool,
    checked: bool,
    /// Every bit proof verified, as the log shows it.
    proven: bool,
    verified: BTreeSet<u16>,
}

impl PhoneLog {
    fn has(&self, matches: impl Fn(&CoordCardMutation) -> bool) -> bool {
        self.log.iter().any(matches)
    }
}

/// Another phone with the same wallet: its app and its persisted state, swapped into a
/// [`World`] with [`World::swap_phone`].
struct Phone {
    coordinator: FrostCoordinator,
    start: FrostCoordinator,
    state: PhoneLog,
}

impl Phone {
    fn new(coordinator: FrostCoordinator) -> Self {
        Self {
            start: coordinator.clone(),
            coordinator,
            state: PhoneLog::default(),
        }
    }
}

/// The phone, the device, the firmware's session, the link, and the user.
struct World {
    run: Run,
    id: DeviceId,
    asref: AccessStructureRef,
    share_index: ShareIndex,
    share_image: ShareImage,
    /// The share's card, for the checks. The user never reads it.
    truth: [u16; WORDS],
    session: Option<CardSession<'static>>,
    /// The jobs' RNG.
    device_rng: DeviceRng,
    /// The firmware's CSPRNG a session opens from.
    fw_rng: DeviceRng,
    phone_rng: StdRng,
    wire: VecDeque<Frame>,
    filter: Option<Filter>,
    /// The protocol's re-send after a `Busy`, dropped once the device answers anything else.
    resend: Option<CoordinatorCard>,
    /// After an unplug or an app restart the user taps Continue by themselves.
    auto_continue: bool,
    /// Owner B9: how the phone's user answers "did you cause the interruption?", or `None` to
    /// leave the question unanswered.
    interruption_answer: Option<bool>,
    /// What the user was doing on the page the question covers: back once they answer "yes".
    covered_actions: Vec<PhoneAction>,

    // The user, at the device.
    screen: Option<CardScreen>,
    acted: bool,
    /// The card as the user wrote it.
    card: [Option<u16>; WORDS],
    /// A lying chip: what its screen shows for these words of 1–32, consistently.
    device_lies: BTreeMap<u8, u16>,
    /// A lying phone: what its screen shows for these words of 33–64, consistently.
    phone_lies: BTreeMap<u8, u16>,
    /// Device-round wrong picks to make at E[k].
    round_misses: BTreeMap<u8, u8>,
    /// The device's user doesn't move past this screen.
    device_holds: Option<fn(&CardScreen) -> bool>,

    // The user, at the phone.
    phone: PhoneLog,
    phone_actions: VecDeque<PhoneAction>,
    auto_final_check: bool,
    /// The phone's user confirms words 1–32 when asked.
    auto_confirm: bool,
    /// Owner B15: on an install that didn't run the keygen, the user chooses to write the card
    /// unverified when asked.
    auto_write_unverified: bool,
    /// The phone's user doesn't take the final check further than this.
    phone_holds: Option<fn(&CardEvent) -> bool>,
    /// The flow's `Begin`, as the protocol keeps it to send again.
    last_begin: Option<CoordinatorSend>,
    /// Wrong picks to make in the phone's own quiz, before the right one.
    quiz_slips: usize,
    /// Wrong answers to give in the phone round.
    misses: usize,

    // What happened.
    events: Vec<CardEvent>,
    /// Frames as delivered, in order.
    frames: Vec<Frame>,
    screens: Vec<CardScreen>,
    /// The coordinator's `Err`s: unexpected or malformed messages, which change nothing.
    errors: Vec<String>,
    sessions_opened: usize,
    /// Every card mutation the device persisted, in order.
    device_log: Vec<DeviceCardMutation>,
    /// Every word position whose word a device screen showed.
    device_seen: BTreeSet<u8>,
    /// Every word position whose word reached the phone, in a frame or on its screen.
    phone_seen: BTreeSet<u8>,
    /// The device's job events, in order.
    job_events: Vec<&'static str>,

    // Cuts.
    cut_at: Option<usize>,
    cuts: Vec<Cut>,
    cut: Option<Cut>,
}

impl World {
    /// The first device's card flow on a `t`-of-`n` card key.
    fn new(n: usize, t: u16, seed: u64) -> Self {
        let run = key(n, t, seed);
        let id = run.device_vec()[0];
        Self::with_run(run, id, seed)
    }

    fn with_run(mut run: Run, id: DeviceId, seed: u64) -> Self {
        let access_structure = run.coordinator.iter_access_structures().next().unwrap();
        let asref = access_structure.access_structure_ref();
        let share_index = access_structure.device_to_share_indicies()[&id];
        let root = run
            .coordinator
            .root_shared_key(asref, TEST_ENCRYPTION_KEY)
            .unwrap();
        let share_image = root.share_image(share_index);
        let contrib = CoordShareDecryptionContrib::for_master_share(id, share_index, &root);
        let share = run
            .device(id)
            .get_encrypted_share(asref, share_index)
            .unwrap()
            .decrypt(asref, contrib, &mut TestDeviceKeyGen)
            .unwrap();
        let truth = codec::encode(&share, &root).unwrap().words;
        Self {
            run,
            id,
            asref,
            share_index,
            share_image,
            truth,
            session: None,
            device_rng: DeviceRng(Some(StdRng::seed_from_u64(seed + 1))),
            fw_rng: DeviceRng(Some(StdRng::seed_from_u64(seed + 3))),
            phone_rng: StdRng::seed_from_u64(seed + 2),
            wire: VecDeque::new(),
            filter: None,
            resend: None,
            auto_continue: true,
            interruption_answer: Some(true),
            covered_actions: vec![],
            screen: None,
            acted: false,
            card: [None; WORDS],
            device_lies: BTreeMap::new(),
            phone_lies: BTreeMap::new(),
            round_misses: BTreeMap::new(),
            device_holds: None,
            phone: PhoneLog::default(),
            phone_actions: VecDeque::from([PhoneAction::Begin]),
            auto_final_check: true,
            auto_confirm: true,
            auto_write_unverified: true,
            phone_holds: None,
            last_begin: None,
            quiz_slips: 0,
            misses: 0,
            events: vec![],
            frames: vec![],
            screens: vec![],
            errors: vec![],
            sessions_opened: 0,
            device_log: vec![],
            device_seen: BTreeSet::new(),
            phone_seen: BTreeSet::new(),
            job_events: vec![],
            cut_at: None,
            cuts: vec![],
            cut: None,
        }
    }

    fn device(&mut self) -> &mut FrostSigner {
        self.run.devices.get_mut(&self.id).unwrap()
    }

    fn record(&mut self) -> Option<CardRecord> {
        let share_image = self.share_image;
        self.device().card_record(&share_image).cloned()
    }

    fn state(&mut self) -> Option<CardState> {
        self.record().map(|rec| rec.state)
    }

    /// `NoCard` for an install that didn't run the keygen and has no record (owner B15).
    fn status(&self) -> CardBackupStatus {
        self.run
            .coordinator
            .card_backup_status(self.asref, self.share_index)
            .unwrap_or(CardBackupStatus::NoCard)
    }

    fn phone_half_in_ram(&self) -> bool {
        self.run
            .coordinator
            .card_phone_half_in_ram(self.asref, self.share_index)
    }

    /// Owner B15: the phone that holds the link ran this key's keygen.
    fn created_here(&self) -> bool {
        self.run.coordinator.card_created_here(self.asref.key_id)
    }

    fn suspect(&self) -> bool {
        self.run.coordinator.device_is_suspect(self.id)
    }

    fn pump(&mut self) {
        self.pump_until(|_| false)
    }

    /// Runs the flow until `until` holds or nothing is left to do: frames first, then the
    /// phone's user, then the device's user, then the device's job, then the re-send after a
    /// `Busy`.
    fn pump_until(&mut self, mut until: impl FnMut(&mut World) -> bool) {
        for _ in 0..2_000_000 {
            if until(self) {
                return;
            }
            if let Some(frame) = self.wire.pop_front() {
                self.deliver(frame);
            } else if let Some(action) = self.phone_actions.pop_front() {
                self.phone_act(action);
            } else if !self.user_act() && !self.step_job() {
                match self.resend.take() {
                    Some(message) => self.wire.push_back(Frame::ToDevice(message)),
                    None => return,
                }
            }
        }
        panic!("the flow doesn't settle");
    }

    /// Records a point where the flow could be cut, and says whether to cut it here.
    fn opportunity(&mut self, cut: Cut) -> bool {
        let here = self.cut_at == Some(self.cuts.len());
        self.cuts.push(cut.clone());
        if here {
            self.cut = Some(cut);
        }
        here
    }

    fn deliver(&mut self, frame: Frame) {
        let before = frame.kind();
        if self.opportunity(Cut::Device { before }) {
            return self.cut_device(0);
        }
        if self.opportunity(Cut::App { before }) {
            return self.cut_app(true);
        }
        let frame = match self.filter.as_mut() {
            Some(filter) => match filter(frame) {
                Tamper::Pass(frame) => frame,
                Tamper::Drop => return,
            },
            None => frame,
        };
        self.frames.push(frame.clone());
        match frame {
            Frame::ToDevice(message) => {
                let answered = matches!(message, CoordinatorCard::BoxAnswered { .. });
                let sends = self
                    .run
                    .devices
                    .get_mut(&self.id)
                    .unwrap()
                    .recv_card_message(message, self.session.as_mut());
                if answered {
                    // T11: no output and no state change, so the dots can't move.
                    assert!(sends.is_empty());
                    assert!(self.device().staged_mutations().is_empty());
                }
                self.device_out(sends);
            }
            Frame::ToPhone(message) => {
                // The words a frame carries reach the phone.
                match &message {
                    DeviceCard::OpeningPart(CardOpeningPart::Carries { j, .. }) => {
                        self.phone_seen.insert(*j);
                    }
                    DeviceCard::PhoneHalf(_) => {
                        self.phone_seen.extend(PHONE_HALF.map(|j| j as u8));
                    }
                    DeviceCard::RecheckWords { d_words, .. } => {
                        self.phone_seen.extend(PHONE_HALF.map(|j| j as u8));
                        if d_words.is_some() {
                            let d = self.record().unwrap().d.unwrap();
                            self.phone_seen.extend(d);
                        }
                    }
                    _ => {}
                }
                let reply = !matches!(
                    message,
                    DeviceCard::Refused {
                        reason: CardRefusal::Busy
                    } | DeviceCard::GateShown
                        | DeviceCard::CheckDone
                        | DeviceCard::DeviceRound { .. }
                        | DeviceCard::RecheckDone
                );
                if reply {
                    self.resend = None;
                }
                match self
                    .run
                    .coordinator
                    .recv_device_message(self.id, DeviceToCoordinatorMessage::Card(message))
                {
                    Ok(sends) => self.phone_out(sends),
                    Err(error) => {
                        assert!(self.run.coordinator.staged_mutations().is_empty());
                        self.errors.push(error.to_string());
                    }
                }
            }
        }
    }

    /// The firmware's handling of one call's result: flush the staged mutations, then send
    /// and show. Gives the number of screens shown, or `None` if the device lost power first.
    fn device_out(&mut self, sends: Vec<DeviceSend>) -> Option<usize> {
        let staged = self
            .device()
            .staged_mutations()
            .iter()
            .map(|mutation| mutation.kind())
            .collect::<Vec<_>>();
        if !staged.is_empty() {
            for persisted in 0..=staged.len() {
                let cut = Cut::DeviceFlush {
                    persisted,
                    staged: staged.clone(),
                };
                if self.opportunity(cut) {
                    self.cut_device(persisted);
                    return None;
                }
            }
        }
        self.flush_device();

        let mut screens = 0;
        for send in sends {
            match send {
                DeviceSend::ToCoordinator(message) => match *message {
                    DeviceToCoordinatorMessage::Card(message) => {
                        if matches!(message, DeviceCard::CheckDone) {
                            assert_eq!(self.flash_state(), Some(CardState::CardChecked));
                        }
                        self.wire.push_back(Frame::ToPhone(message))
                    }
                    other => panic!("unexpected {other:?}"),
                },
                DeviceSend::ToUser(message) => match *message {
                    DeviceToUserMessage::Card(message) => match *message {
                        ToUserCard::OpenSession(start) => {
                            // Two sessions never coexist.
                            self.session = None;
                            let mut session = start
                                .open(&mut TestDeviceKeyGen, card_setup(), &mut self.fw_rng)
                                .unwrap();
                            assert!(self.device().card_session_opened(&mut session).is_empty());
                            assert!(self.device().staged_mutations().is_empty());
                            self.session = Some(session);
                            self.sessions_opened += 1;
                        }
                        ToUserCard::Screen(screen) => {
                            screens += 1;
                            self.show(screen);
                        }
                        ToUserCard::CloseSession => self.session = None,
                        other => panic!("unexpected {other:?}"),
                    },
                    other => panic!("unexpected {other:?}"),
                },
            }
        }
        assert!(screens <= 1, "one screen per step");
        Some(screens)
    }

    /// The record on the device's flash.
    fn flash_state(&self) -> Option<CardState> {
        self.run.start_devices[&self.id]
            .card_record(&self.share_image)
            .map(|rec| rec.state.clone())
    }

    /// The device's screen, as its (possibly lying) chip draws it.
    fn show(&mut self, mut screen: CardScreen) {
        // The dots are the persisted count, after a resume too.
        if let CardScreen::Gate { dots, .. } = &screen {
            assert_eq!(*dots, self.record().unwrap().dots())
        }
        // Persist before output (design §3.10): each screen's state is on flash already.
        let persisted = match &screen {
            CardScreen::DeviceHalf { .. } => Some(CardState::Writing),
            CardScreen::SelfCheck { .. } => Some(CardState::SelfCheck),
            CardScreen::PhoneHalfNext => Some(CardState::CardChecked),
            CardScreen::Preparing {
                task: BusyTask::Proofs,
            } => Some(CardState::Proving),
            CardScreen::DeviceRoundIntro | CardScreen::DeviceRoundAnswer { .. } => {
                Some(CardState::Checking)
            }
            _ => None,
        };
        if let Some(state) = persisted {
            assert_eq!(
                self.flash_state(),
                Some(state),
                "{screen:?} before its state"
            );
        }
        match &mut screen {
            CardScreen::DeviceHalf { words } => {
                self.device_seen.extend(DEVICE_HALF.map(|j| j as u8));
                for (j, word) in words.0.iter_mut().enumerate() {
                    if let Some(lie) = self.device_lies.get(&(j as u8)) {
                        *word = *lie;
                    }
                }
            }
            CardScreen::SelfCheck { items, .. } | CardScreen::Recheck { items, .. } => {
                for (number, word) in items.0.iter_mut() {
                    let j = *number - 1;
                    self.device_seen.insert(j);
                    if let Some(lie) = self.device_lies.get(&j) {
                        *word = *lie;
                    }
                }
            }
            CardScreen::DeviceRoundAnswer { j, .. } => {
                self.device_seen.insert(*j);
            }
            _ => {}
        }
        self.screens.push(screen.clone());
        self.screen = Some(screen);
        self.acted = false;
    }

    fn flush_device(&mut self) {
        let id = self.id;
        let staged = self
            .device()
            .staged_mutations()
            .drain(..)
            .collect::<Vec<_>>();
        let flash = self.run.start_devices.get_mut(&id).unwrap();
        for mutation in staged {
            if let DeviceMutation::Card(card) = &mutation {
                self.device_log.push(card.clone());
            }
            flash.apply_mutation(mutation);
        }
    }

    /// The device loses power with `persisted` of its staged mutations on flash: it's rebuilt
    /// from flash, the session and the screen are gone. The user taps Continue, if
    /// `auto_continue`.
    fn cut_device(&mut self, persisted: usize) {
        let id = self.id;
        let staged = self
            .device()
            .staged_mutations()
            .drain(..)
            .collect::<Vec<_>>();
        let flash = self.run.start_devices.get_mut(&id).unwrap();
        for mutation in staged.into_iter().take(persisted) {
            if let DeviceMutation::Card(card) = &mutation {
                self.device_log.push(card.clone());
            }
            flash.apply_mutation(mutation);
        }
        let mut device = flash.clone();
        *device.nonce_slots() = self.device().nonce_slots().clone();
        self.run.devices.insert(id, device);
        self.session = None;
        self.screen = None;
        self.wire.clear();
        self.resend = None;
        self.unplugged();
    }

    /// The app's loop on an unplug. Owner B9: while the D check runs it's an interruption,
    /// persisted, and the user is asked whether they caused it; the flow goes on (Continue)
    /// only after "yes". Otherwise the user taps Continue by themselves, if `auto_continue`.
    fn unplugged(&mut self) {
        let sends = self.run.coordinator.card_device_disconnected(self.id);
        self.phone_actions
            .retain(|action| !matches!(action, PhoneAction::Begin));
        let asked = sends.iter().any(|send| {
            matches!(
                send,
                CoordinatorSend::ToUser(CoordinatorToUserMessage::Card(CardToUser {
                    event: CardEvent::Interrupted { .. },
                    ..
                }))
            )
        });
        self.phone_out(sends);
        if self.auto_continue && !asked {
            self.phone_actions.push_back(PhoneAction::Begin);
        }
    }

    /// The app restarts: the coordinator is rebuilt from its log (with this call's mutations
    /// if `persisted`), and the user opens the card flow again. The device's soft reset ends
    /// its session and sends it home, as the firmware does.
    fn cut_app(&mut self, persisted: bool) {
        if persisted {
            self.flush_phone();
        } else {
            self.run.coordinator.take_staged_mutations();
        }
        self.run.coordinator = self.run.start_coordinator.clone();
        self.device().clear_tmp_data();
        self.session = None;
        self.screen = None;
        self.wire.clear();
        self.resend = None;
        self.phone_actions.clear();
        self.covered_actions.clear();
        if self.auto_continue {
            self.phone_actions.push_back(PhoneAction::Begin);
        }
    }

    /// The app persists what the coordinator staged, checking the order of the rows as it goes.
    fn flush_phone(&mut self) {
        let created_here = self.created_here();
        for mutation in self.run.coordinator.take_staged_mutations() {
            if let CoordMutation::Card(card) = &mutation {
                // Owner B15: an install that didn't run the keygen records no step of the
                // verified check: no pin, no N, no proof, no opening, no half of the check.
                if !created_here {
                    assert!(
                        matches!(
                            card,
                            CoordCardMutation::UnverifiedWriteStarted { .. }
                                | CoordCardMutation::UnverifiedWritten { .. }
                                | CoordCardMutation::Failed { .. }
                                | CoordCardMutation::DeviceVerdict { passed: false, .. }
                                | CoordCardMutation::DeviceSuspect { .. }
                        ),
                        "{card:?} on an install that didn't run the keygen"
                    );
                }
                match card {
                    CoordCardMutation::Finished { .. } => self.phone.finished = true,
                    CoordCardMutation::DeviceHalfWritten { .. } => {
                        assert!(self.phone.finished, "DeviceHalfWritten before Finished");
                        self.phone.device_half_written = true;
                    }
                    CoordCardMutation::ProofsVerified { from, count, .. } => {
                        assert!(
                            self.phone.device_half_written,
                            "a proof before the user confirmed words 1–32"
                        );
                        self.phone.verified.extend(*from..*from + *count as u16);
                        self.phone.proven = self.phone.verified.len() == COMMIT_COUNT as usize;
                    }
                    CoordCardMutation::Checked { .. } => {
                        assert!(
                            self.phone.device_half_written,
                            "Checked before DeviceHalfWritten"
                        );
                        self.phone.checked = true;
                    }
                    // The device round starts at [Continue] (owner B1): both halves recorded,
                    // the seal proven. Owner B8: on the N drawn with the confirm of words 1–32.
                    CoordCardMutation::DeviceRoundPending { n, .. } => {
                        assert!(
                            self.phone.checked && self.phone.proven,
                            "the device round before [Continue] was possible"
                        );
                        assert_eq!(self.phone.n, Some(n.0), "the round runs on the early N");
                        self.phone.pending_n = Some(n.0)
                    }
                    CoordCardMutation::DeviceRoundResult { pass: true, .. } => {
                        self.phone.round_passed = true
                    }
                    // Owner B8: N is drawn and persisted with the confirm of words 1–32, once
                    // the device's quiz passed, and before the device round.
                    CoordCardMutation::CheckStarted { n, .. } => {
                        assert!(
                            self.phone.finished && self.phone.device_half_written,
                            "CheckStarted before the confirm of words 1–32"
                        );
                        assert!(!self.phone.round_passed, "N drawn after the device round");
                        self.phone.n = Some(n.0);
                    }
                    CoordCardMutation::BoxesDrawn { d, .. } => self.phone.d = Some(*d),
                    _ => {}
                }
                self.phone.log.push(card.clone());
            }
            self.run.start_coordinator.apply_mutation(mutation);
        }
        // Checked needs all three keys: the device round, the phone round and the device's
        // verdict.
        if self.status() == CardBackupStatus::Checked {
            assert!(self.phone.round_passed);
            assert!(self
                .phone
                .has(|m| matches!(m, CoordCardMutation::RoundResult { pass: true, .. })));
            assert!(self
                .phone
                .has(|m| matches!(m, CoordCardMutation::DeviceVerdict { passed: true, .. })));
        }
    }

    /// The app's handling of a coordinator result: persist, then send and show.
    fn phone_out(&mut self, sends: Vec<CoordinatorSend>) {
        let staged = self
            .run
            .coordinator
            .staged_mutations()
            .iter()
            .map(|mutation| mutation.kind())
            .collect::<Vec<_>>();
        if !staged.is_empty() && self.opportunity(Cut::AppPersist { staged }) {
            return self.cut_app(false);
        }
        self.flush_phone();
        for send in sends {
            match send {
                CoordinatorSend::ToDevice {
                    message: CoordinatorToDeviceMessage::Card(message),
                    destinations,
                } => {
                    assert_eq!(destinations, [self.id].into());
                    // Neither side alone starts the proofs (owner B1): the phone pulls them only
                    // once the user's confirm of words 1–32 is persisted.
                    if matches!(
                        message,
                        CoordinatorCard::PullProofs { .. }
                            | CoordinatorCard::DeviceHalfWritten { .. }
                    ) {
                        assert!(
                            self.phone.device_half_written,
                            "{message:?} before the confirm"
                        );
                    }
                    // Owner B15: an install that didn't run the keygen sends only what writing
                    // the card takes, and a later check: never a pull of the seal, a proof, an
                    // opening, N or the device round.
                    if !self.created_here() {
                        assert!(
                            matches!(
                                message,
                                CoordinatorCard::Begin(_)
                                    | CoordinatorCard::PullPhoneHalf { .. }
                                    | CoordinatorCard::WriteStart { .. }
                                    | CoordinatorCard::Recheck { .. }
                            ),
                            "{message:?} from an install that didn't run the keygen"
                        );
                    }
                    if matches!(message, CoordinatorCard::WriteStart { .. }) {
                        assert!(
                            self.phone.has(|m| matches!(
                                m,
                                CoordCardMutation::Pinned { .. }
                                    | CoordCardMutation::UnverifiedWriteStarted { .. }
                            )),
                            "WriteStart before the pin, or the user's choice to write unverified"
                        );
                    }
                    // Owner B8: N leaves the phone only once it's persisted, after the confirm of
                    // words 1–32.
                    if matches!(message, CoordinatorCard::OpenStart { .. }) {
                        assert!(
                            self.phone.n.is_some() && self.phone.device_half_written,
                            "N left the phone before it was persisted"
                        );
                    }
                    self.wire.push_back(Frame::ToDevice(message));
                }
                CoordinatorSend::ToUser(CoordinatorToUserMessage::Card(CardToUser {
                    device_id,
                    event,
                    ..
                })) => {
                    assert_eq!(device_id, self.id);
                    // Words 33–64 show on the phone only after the confirm (owner B1). Owner
                    // B15: on an unverified card the confirm isn't persisted, but it's taken only
                    // once the device's quiz on words 1–32 passed.
                    if matches!(event, CardEvent::PhonePage { .. })
                        || matches!(event, CardEvent::SelfCheckQuestion { recheck: false, .. })
                    {
                        let unverified_confirm = !self.created_here()
                            && self.flash_state() == Some(CardState::CardChecked);
                        assert!(
                            self.phone.device_half_written || unverified_confirm,
                            "a word of 33–64 before the confirm"
                        );
                    }
                    // A word of D shows on the phone only while the device shows its gate
                    // screen (spec §0.6.1.2 item 2).
                    if matches!(event, CardEvent::Question { .. }) {
                        assert!(
                            matches!(self.screen, Some(CardScreen::Gate { .. })),
                            "a word on the phone while the device shows {:?}",
                            self.screen
                        );
                    }
                    self.phone_reacts(&event);
                    self.events.push(event);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    /// What the phone shows for word j of 33–64: a lying phone's word, or core's.
    fn phone_shows(&self, j: u8, word: &str) -> u16 {
        self.phone_lies
            .get(&j)
            .copied()
            .unwrap_or_else(|| word_index(word))
    }

    fn phone_reacts(&mut self, event: &CardEvent) {
        if self.phone_holds.is_some_and(|holds| holds(event)) {
            return;
        }
        match event {
            CardEvent::PhonePage {
                page,
                first_word,
                words,
            } => {
                for (i, word) in words.iter().enumerate() {
                    let j = first_word - 1 + i as u8;
                    assert!(PHONE_HALF.contains(&(j as usize)));
                    self.phone_seen.insert(j);
                    let shown = self.phone_shows(j, word);
                    self.card[j as usize].get_or_insert(shown);
                }
                let next = if *page < 11 {
                    PhoneAction::Page(page + 1)
                } else {
                    PhoneAction::HalfWritten
                };
                // The user acts on the page on screen: one shown again (a resume) replaces it.
                self.phone_actions.retain(|action| {
                    !matches!(action, PhoneAction::Page(_) | PhoneAction::HalfWritten)
                });
                self.phone_actions.push_back(next);
            }
            CardEvent::SelfCheckQuestion {
                word_number,
                word,
                lookalikes,
                ..
            } => {
                let j = word_number - 1;
                self.phone_seen.insert(j);
                let pick = if self.quiz_slips > 0 {
                    self.quiz_slips -= 1;
                    word_index(lookalikes[0])
                } else {
                    // A lying phone's quiz passes the word it showed: core's is the true one.
                    word_index(word)
                };
                self.phone_actions
                    .retain(|action| !matches!(action, PhoneAction::SelfCheck(_)));
                self.phone_actions.push_back(PhoneAction::SelfCheck(pick));
            }
            CardEvent::ReadyForFinalCheck if self.auto_final_check => {
                self.phone_actions.push_back(PhoneAction::FinalCheck)
            }
            CardEvent::UnverifiedIntro if self.auto_write_unverified => {
                self.phone_actions
                    .retain(|action| !matches!(action, PhoneAction::WriteUnverified));
                self.phone_actions.push_back(PhoneAction::WriteUnverified)
            }
            CardEvent::ConfirmDeviceHalf if self.auto_confirm => {
                self.phone_actions
                    .retain(|action| !matches!(action, PhoneAction::ConfirmDeviceHalf));
                self.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf)
            }
            CardEvent::Question {
                word_number,
                word,
                pool,
                ..
            } => {
                let j = word_number - 1;
                self.phone_seen.insert(j);
                // The protocol picks two of the six; the user's card word is shown or not.
                let options = [word_index(word), word_index(pool[0]), word_index(pool[1])];
                let written = self.card[j as usize];
                let answer = if self.misses > 0 {
                    self.misses -= 1;
                    BoxAnswer::NoneOfThese
                } else {
                    match written.filter(|w| options.contains(w)) {
                        Some(w) => BoxAnswer::Picked(w),
                        None => BoxAnswer::NoneOfThese,
                    }
                };
                self.phone_actions.push_back(PhoneAction::Answer(answer));
            }
            CardEvent::Busy { resend } => match resend.as_ref() {
                CoordinatorToDeviceMessage::Card(message) => self.resend = Some(message.clone()),
                other => panic!("unexpected {other:?}"),
            },
            CardEvent::NeedsBegin => self.phone_actions.push_back(PhoneAction::Begin),
            CardEvent::Interrupted { .. } => self.ask_about_interruption(),
            // Owner B9: "yes": a page of words in RAM is back (any other page waits for the
            // device, which shows it again), and the flow sends `Begin` again.
            CardEvent::Resuming => {
                let covered = self.covered_actions.drain(..).filter(|action| {
                    matches!(
                        action,
                        PhoneAction::Page(_) | PhoneAction::HalfWritten | PhoneAction::SelfCheck(_)
                    )
                });
                self.phone_actions.extend(covered);
                if self.auto_continue {
                    self.phone_actions
                        .retain(|action| !matches!(action, PhoneAction::Begin));
                    self.phone_actions.push_back(PhoneAction::Begin);
                }
            }
            _ => {}
        }
    }

    /// Owner B9: the phone asks whether the user caused the interruption. The question
    /// covers the page, so it's answered before anything else the user does there.
    fn ask_about_interruption(&mut self) {
        self.phone_actions
            .retain(|action| !matches!(action, PhoneAction::AnswerInterruption(_)));
        if let Some(answer) = self.interruption_answer {
            self.phone_actions
                .push_front(PhoneAction::AnswerInterruption(answer));
        }
    }

    fn phone_act(&mut self, action: PhoneAction) {
        // Owner B9: the question covers the page, so what the user was doing there waits.
        if self.run.coordinator.card_interruption_pending(self.id)
            && !matches!(
                action,
                PhoneAction::AnswerInterruption(_) | PhoneAction::Begin | PhoneAction::Replug
            )
        {
            return self.covered_actions.push(action);
        }
        let (id, coord) = (self.id, &mut self.run.coordinator);
        let sends = match action {
            PhoneAction::Begin => {
                let begin = coord
                    .card_begin(id, self.asref, TEST_ENCRYPTION_KEY, &mut self.phone_rng)
                    .unwrap();
                self.last_begin = Some(begin.clone());
                // Owner B9: a check interrupted by an app restart asks first, and the flow sends
                // its `Begin` only after "yes".
                if coord.card_interruption_pending(id) {
                    self.phone_out(vec![]);
                    self.ask_about_interruption();
                    return;
                }
                vec![begin]
            }
            PhoneAction::Replug => vec![self.last_begin.clone().expect("a flow")],
            PhoneAction::ConfirmDeviceHalf => {
                match coord.card_device_half_written(id, &mut self.phone_rng) {
                    Ok(sends) => sends,
                    Err(error) => return self.errors.push(error.to_string()),
                }
            }
            PhoneAction::Page(page) => match coord.card_phone_page(id, page) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
            PhoneAction::HalfWritten => match coord.card_phone_half_written(id) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
            PhoneAction::SelfCheck(pick) => match coord.card_self_check_answer(id, pick) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
            // The device round starts once; a resume re-sends it from the Status.
            PhoneAction::FinalCheck if self.phone.round_passed => return,
            PhoneAction::FinalCheck => match coord.card_final_begin(id, &mut self.phone_rng) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
            PhoneAction::Answer(answer) => match coord.card_answer(id, answer) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
            PhoneAction::AnswerInterruption(caused) => {
                match coord.card_interruption_answer(id, caused) {
                    Ok(sends) => sends,
                    Err(error) => return self.errors.push(error.to_string()),
                }
            }
            PhoneAction::WriteUnverified => match coord.card_write_unverified(id) {
                Ok(sends) => sends,
                Err(error) => return self.errors.push(error.to_string()),
            },
        };
        self.phone_out(sends);
    }

    /// The device's user acts on the screen, if it asks for anything.
    fn user_act(&mut self) -> bool {
        let Some(screen) = self.screen.clone() else {
            return false;
        };
        // A verdict screen needs no session; every other screen does.
        let verdict = matches!(
            screen,
            CardScreen::Verdict { .. }
                | CardScreen::PhoneRoundFailed
                | CardScreen::Stopped
                | CardScreen::DeviceRoundFailed
        );
        if self.acted || (self.session.is_none() && !verdict) {
            return false;
        }
        if self.device_holds.is_some_and(|holds| holds(&screen)) {
            return false;
        }
        let action = match screen {
            CardScreen::DeviceHalf { words } => {
                for (j, word) in words.0.iter().enumerate() {
                    self.card[j].get_or_insert(*word);
                }
                CardUserAction::DeviceHalfWritten
            }
            // The standard check quiz: retry until right. A slip is fixed on the card.
            CardScreen::SelfCheck { items, .. } => {
                assert_eq!(items.0.len(), HALF);
                for &(number, word) in &items.0 {
                    self.card[number as usize - 1] = Some(word);
                }
                CardUserAction::SelfCheckPassed
            }
            CardScreen::Recheck { items, .. } => {
                assert_eq!(items.0.len(), HALF + CHECK_BOXES);
                for &(number, word) in &items.0 {
                    assert_eq!(self.card[number as usize - 1], Some(word));
                }
                CardUserAction::RecheckPassed
            }
            CardScreen::DeviceRoundIntro => CardUserAction::DeviceRoundIntroDone,
            CardScreen::DeviceRoundAnswer {
                j,
                k,
                options,
                display_id,
            } => {
                let written = self.card[j as usize];
                let right = options
                    .0
                    .iter()
                    .position(|&o| Some(o) == written)
                    .map(|i| i as u8);
                let pick = match self.round_misses.get_mut(&k).filter(|left| **left > 0) {
                    Some(left) => {
                        *left -= 1;
                        (0..3).find(|&i| Some(i) != right)
                    }
                    None => right,
                };
                CardUserAction::DeviceRoundPick { display_id, pick }
            }
            CardScreen::DeviceRoundMismatch { .. } => CardUserAction::DeviceRoundMismatchAck,
            CardScreen::Verdict { passed: true } => CardUserAction::VerdictAck,
            CardScreen::PhoneRoundFailed | CardScreen::Stopped | CardScreen::DeviceRoundFailed => {
                CardUserAction::FailAck
            }
            _ => return false,
        };
        self.device_act(action)
    }

    fn device_act(&mut self, action: CardUserAction) -> bool {
        self.acted = true;
        let sends = self
            .run
            .devices
            .get_mut(&self.id)
            .unwrap()
            .card_user_action(self.session.as_mut(), action);
        self.device_out(sends);
        true
    }

    /// Runs the device's job until it reports. Spec PK.7: while the record is WRITING,
    /// SELF_CHECK or CARD_CHECKED only the encode may run, and the bit proofs run only in
    /// PROVING.
    fn step_job(&mut self) -> bool {
        let state = self.state();
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let Some(progress) = session.progress() else {
            return false;
        };
        let writing = matches!(
            state,
            Some(CardState::Writing | CardState::SelfCheck | CardState::CardChecked)
        );
        assert!(
            !writing || progress.task == BusyTask::Card,
            "a {:?} job in {state:?}",
            progress.task
        );
        if progress.task == BusyTask::Proofs {
            assert_eq!(state, Some(CardState::Proving), "proofs outside PROVING");
        }
        let event = loop {
            if let Some(event) = session.step(&mut self.device_rng) {
                break event;
            }
        };
        self.job_events.push(match &event {
            CardJobEvent::Encoded => "Encoded",
            CardJobEvent::Sealed(_) => "Sealed",
            CardJobEvent::ProofsDone => "ProofsDone",
            CardJobEvent::OpeningReady { .. } => "OpeningReady",
            CardJobEvent::Failed(_) => "Failed",
        });
        let sends = self
            .run
            .devices
            .get_mut(&self.id)
            .unwrap()
            .card_job_event(session, event, true);
        self.device_out(sends);
        true
    }

    /// The device's flash replaced by `flash` with `log` replayed on it (a chip that rewrote
    /// its record), then a power cut: it boots from the new flash, and the user taps Continue.
    fn replace_device(&mut self, mut flash: FrostSigner, log: Vec<DeviceCardMutation>) {
        for mutation in log {
            flash.apply_mutation(DeviceMutation::Card(mutation));
        }
        *flash.nonce_slots() = self.device().nonce_slots().clone();
        self.device().staged_mutations().clear();
        self.run.start_devices.insert(self.id, flash.clone());
        self.run.devices.insert(self.id, flash);
        self.session = None;
        self.screen = None;
        self.wire.clear();
        self.resend = None;
        self.unplugged();
    }

    /// A message from another phone, straight to the device; gives the device's replies.
    /// `PullPhoneHalf` for the device's own seal, as an honest phone names it.
    fn pull_phone_half(&mut self) -> Vec<DeviceCard> {
        let h_c = self.record().unwrap().sealed.unwrap().h_c;
        self.device_replies(CoordinatorCard::PullPhoneHalf { h_c })
    }

    fn device_replies(&mut self, message: CoordinatorCard) -> Vec<DeviceCard> {
        assert!(self.wire.is_empty());
        let sends = self
            .run
            .devices
            .get_mut(&self.id)
            .unwrap()
            .recv_card_message(message, self.session.as_mut());
        self.device_out(sends);
        self.wire
            .drain(..)
            .map(|frame| match frame {
                Frame::ToPhone(message) => message,
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn sent_to_device(&self, matches: impl Fn(&CoordinatorCard) -> bool) -> bool {
        self.frames
            .iter()
            .any(|frame| matches!(frame, Frame::ToDevice(message) if matches(message)))
    }

    fn sent_to_phone(&self, matches: impl Fn(&DeviceCard) -> bool) -> bool {
        self.frames
            .iter()
            .any(|frame| matches!(frame, Frame::ToPhone(message) if matches(message)))
    }

    fn count_to_device(&self, matches: impl Fn(&CoordinatorCard) -> bool) -> usize {
        self.frames
            .iter()
            .filter(|frame| matches!(frame, Frame::ToDevice(message) if matches(message)))
            .count()
    }

    /// Swaps the app and its persisted state for `phone`'s: another phone, with the same
    /// wallet, now holds the link.
    fn swap_phone(&mut self, phone: &mut Phone) {
        std::mem::swap(&mut self.run.coordinator, &mut phone.coordinator);
        std::mem::swap(&mut self.run.start_coordinator, &mut phone.start);
        std::mem::swap(&mut self.phone, &mut phone.state);
        self.resend = None;
    }

    /// The device's E and D, from its record.
    fn e_and_d(&mut self) -> ([u8; CHECK_BOXES], [u8; CHECK_BOXES]) {
        let rec = self.record().unwrap();
        (rec.device_round.unwrap().e, rec.d.unwrap())
    }

    /// The 37-word bound (design §0): the device screen saw only words 1–32 and E, the phone
    /// only words 33–64 and D.
    fn assert_bound(&mut self, context: &str) {
        let rec = self.record().unwrap();
        let mut device_allowed: BTreeSet<u8> = DEVICE_HALF.map(|j| j as u8).collect();
        if let Some(dr) = rec.device_round {
            device_allowed.extend(dr.e);
        }
        let mut phone_allowed: BTreeSet<u8> = PHONE_HALF.map(|j| j as u8).collect();
        if let Some(d) = rec.d {
            phone_allowed.extend(d);
        }
        assert!(
            self.device_seen.is_subset(&device_allowed),
            "{context}: the device saw {:?}",
            self.device_seen
        );
        assert!(
            self.phone_seen.is_subset(&phone_allowed),
            "{context}: the phone saw {:?}",
            self.phone_seen
        );
        assert!(self.device_seen.len() <= 37, "{context}");
        assert!(self.phone_seen.len() <= 37, "{context}");
    }

    /// Owner B15: the first device's card flow, on another app install that holds the wallet
    /// but didn't run its keygen ([`other_install`]). The keygen's phone is never used.
    fn on_other_install(n: usize, t: u16, seed: u64) -> Self {
        let mut w = World::new(n, t, seed);
        let mut other = other_install(n, t, seed);
        assert_eq!(
            other
                .coordinator
                .iter_access_structures()
                .next()
                .unwrap()
                .access_structure_ref(),
            w.asref,
            "the same wallet"
        );
        w.swap_phone(&mut other);
        w
    }

    /// Owner B15: a card written on an install that didn't run the keygen. Both quizzes passed
    /// and the card as written is the share's, but it's "written, not verified": no pin, no N,
    /// no proof or opening, never Checked, and the device is left in CARD_CHECKED holding no N.
    /// Each screen saw exactly its own 32 words.
    fn assert_unverified(&mut self, context: &str) {
        assert!(!self.created_here(), "{context}");
        assert_eq!(self.status(), CardBackupStatus::Unverified, "{context}");
        assert!(
            matches!(self.events.last(), Some(CardEvent::UnverifiedWritten)),
            "{context}: {:?}",
            self.events.last()
        );
        assert!(!self.suspect(), "{context}");
        assert!(self.errors.is_empty(), "{context}: {:?}", self.errors);
        let rec = self.record().expect("a card record");
        assert_eq!(rec.state, CardState::CardChecked, "{context}");
        assert_eq!(rec.n_tag, None, "{context}: the device holds no N");
        assert_eq!(rec.d, None, "{context}: no D exists");
        assert!(
            !self
                .job_events
                .iter()
                .any(|e| matches!(*e, "ProofsDone" | "OpeningReady")),
            "{context}: {:?}",
            self.job_events
        );
        assert!(
            !self.sent_to_device(|m| !matches!(
                m,
                CoordinatorCard::Begin(_)
                    | CoordinatorCard::PullPhoneHalf { .. }
                    | CoordinatorCard::WriteStart { .. }
            )),
            "{context}: only what writing the card takes"
        );
        assert!(
            self.phone.log.iter().all(|m| matches!(
                m,
                CoordCardMutation::UnverifiedWriteStarted { .. }
                    | CoordCardMutation::UnverifiedWritten { .. }
            )),
            "{context}: {:?}",
            self.phone.log
        );
        assert_eq!(
            self.card,
            self.truth.map(Some),
            "{context}: the card as written"
        );
        let device_half: BTreeSet<u8> = DEVICE_HALF.map(|j| j as u8).collect();
        let phone_half: BTreeSet<u8> = PHONE_HALF.map(|j| j as u8).collect();
        assert_eq!(self.device_seen, device_half, "{context}: the device saw");
        assert_eq!(self.phone_seen, phone_half, "{context}: the phone saw");
    }

    fn assert_passed(&mut self, context: &str) {
        let rec = self.record().expect("a card record");
        assert_eq!(rec.state, CardState::Passed, "{context}");
        assert_eq!(self.status(), CardBackupStatus::Checked, "{context}");
        assert!(!self.suspect(), "{context}");
        assert!(
            matches!(self.events.last(), Some(CardEvent::Passed)),
            "{context}: {:?}",
            self.events.last()
        );
        assert_eq!(rec.d, self.phone.d, "{context}: both sides have the same D");
        assert_eq!(rec.dots(), 5, "{context}");
        assert!(rec.device_round_passed(), "{context}");
        assert!(self.errors.is_empty(), "{context}: {:?}", self.errors);
        // The card, written off the two screens, is the share's.
        assert_eq!(
            self.card,
            self.truth.map(Some),
            "{context}: the card as written"
        );
        assert!(self.phone.checked, "{context}");
        self.assert_bound(context);
    }

    /// An abort after N: the phone failed the card and marked the device, and the device
    /// failed once `PhoneRound{false}` arrived.
    fn assert_aborted(&mut self, reason: CardFailReason) {
        let context = format!("{reason:?}");
        assert_eq!(self.status(), CardBackupStatus::Failed, "{context}");
        assert!(self.suspect(), "{context}");
        assert!(
            self.events.iter().any(|event| matches!(
                event,
                CardEvent::Failed { reason: r, device_suspect: true, .. }
                    if *r == reason
            )),
            "{context}: {:?}",
            self.events
        );
        assert!(
            self.sent_to_device(|m| matches!(m, CoordinatorCard::PhoneRound { pass: false, .. })),
            "{context}"
        );
        assert_eq!(
            self.state(),
            Some(CardState::Failed(VerdictReason::PhoneRoundFailed)),
            "{context}"
        );
    }
}

/// Runs `f(0..n)` on every core.
fn parallel(n: usize, f: impl Fn(usize) + Sync) {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(n) {
            std::thread::Builder::new()
                .stack_size(16 << 20)
                .spawn_scoped(scope, || loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    f(i);
                })
                .unwrap();
        }
    });
}

/// Design §3.11 (backup, check, bound), spec PK.7 with owner B1, B2, B6: a 2-of-2 card written in
/// order. The seal and words 33–64 (one `PhoneHalf` frame) come before word 1; words 1–32 on the
/// device and its quiz; the confirm on the phone; words 33–64 on the phone while the device
/// proves, showing no word; [Continue]; the device round on 5 of words 33–64 and the phone round
/// on 5 of words 1–32, each a carry opening. Each screen sees exactly 37 words.
#[test]
fn happy_path() {
    let mut w = World::new(2, 2, 1);
    w.quiz_slips = 2;
    w.pump();
    w.assert_passed("happy path");
    let (e, d) = w.e_and_d();
    assert!(e.iter().all(|&j| PHONE_HALF.contains(&(j as usize))));
    assert!(d.iter().all(|&j| DEVICE_HALF.contains(&(j as usize))));
    assert_eq!(w.device_seen.len(), 37, "words 1–32 and E");
    assert_eq!(w.phone_seen.len(), 37, "words 33–64 and D");
    // The order of the stages on the device: the second "Preparing" is the proving screen.
    let order = [
        "Preparing",
        "WaitingForPhone",
        "DeviceHalf",
        "SelfCheck",
        "PhoneHalfNext",
        "Preparing",
        "CardFinishedWaiting",
        "DeviceRoundIntro",
        "DeviceRoundAnswer",
        "Gate",
        "Verdict",
        "CardDone",
    ];
    let shown: Vec<String> = w.screens.iter().map(|s| format!("{s:?}")).collect();
    let mut at = 0;
    for name in order {
        at += shown[at..]
            .iter()
            .position(|s| s == name)
            .unwrap_or_else(|| panic!("{name} after {:?}", &shown[..at]));
    }
    // Owner B2: from the confirm to the device round, the proving screen and no word.
    let proving = w
        .screens
        .iter()
        .position(|s| {
            matches!(
                s,
                CardScreen::Preparing {
                    task: BusyTask::Proofs
                }
            )
        })
        .expect("the proving screen");
    let intro = shown.iter().position(|s| s == "DeviceRoundIntro").unwrap();
    assert!(w.screens[proving..intro].iter().all(|s| matches!(
        s,
        CardScreen::Preparing { .. } | CardScreen::CardFinishedWaiting
    )));
    // The requests, in order: words 33–64 once, before `WriteStart`; the confirm; the proofs;
    // `FinalBegin`. No phone-half opening and no prepare (spec PK.5).
    let kinds: Vec<&str> = w
        .frames
        .iter()
        .filter_map(|f| match f {
            Frame::ToDevice(m) => Some(m.kind()),
            _ => None,
        })
        .collect();
    let first = |kind: &str| {
        kinds
            .iter()
            .position(|k| *k == kind)
            .unwrap_or_else(|| panic!("{kind}"))
    };
    assert_eq!(
        w.count_to_device(|m| matches!(m, CoordinatorCard::PullPhoneHalf { .. })),
        1
    );
    assert!(first("PullCommit") < first("PullPhoneHalf"));
    assert!(first("PullPhoneHalf") < first("WriteStart"));
    assert!(first("WriteStart") < first("DeviceHalfWritten"));
    assert!(first("DeviceHalfWritten") < first("PullProofs"));
    assert!(first("PullProofs") < first("FinalBegin"));
    assert!(!w.sent_to_device(|m| matches!(
        m,
        CoordinatorCard::OpenPhoneWord { .. } | CoordinatorCard::Prepare { .. }
    )));
    // 311 bit proofs, every one verified once.
    let proofs: usize = w
        .frames
        .iter()
        .map(|f| match f {
            Frame::ToPhone(DeviceCard::BitProofs { proofs, .. }) => proofs.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(proofs, COMMIT_COUNT as usize);
    // No word of 33–64 is ever opened; each word of D once.
    let rec = w.record().unwrap();
    assert!(rec.runs[PHONE_HALF].iter().all(|&runs| runs == 0));
    for &j in &d {
        assert_eq!(rec.runs[j as usize], 1);
    }
    // The phone: the confirm, then words 33–64.
    let confirm = w
        .events
        .iter()
        .position(|e| matches!(e, CardEvent::ConfirmDeviceHalf))
        .expect("the confirm");
    let page = w
        .events
        .iter()
        .position(|e| matches!(e, CardEvent::PhonePage { .. }))
        .expect("a page");
    assert!(confirm < page);
    // A pass goes from the phone round straight to "Backup checked", one success page as in
    // the standard backup check.
    let last_gate = shown.iter().rposition(|s| s == "Gate").unwrap();
    assert_eq!(shown[last_gate + 1], "Verdict");
    // Both halves recorded: Checked{h32}, then the round's h37 replaces it.
    let coord_rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap()
        .clone();
    assert!(coord_rec.halves_recorded());
    let phone_half: [u16; HALF] = w.truth[PHONE_HALF].try_into().unwrap();
    let d_words = d.map(|j| w.truth[j as usize]);
    let h_c = rec.sealed.unwrap().h_c;
    assert_eq!(
        coord_rec.words_hash,
        Some(phone::h37(&h_c, &phone_half, &d, &d_words))
    );
    assert!(w.phone.has(|m| matches!(
        m,
        CoordCardMutation::Checked { words_hash, .. }
            if *words_hash == phone::h32(&h_c, &phone_half)
    )));
    w.run.check_mutations();
}

#[test]
fn one_of_one() {
    let mut w = World::new(1, 1, 2);
    w.pump();
    w.assert_passed("1-of-1");
}

/// Design §3.11 #8: an unplug or an app restart at a spread of points through the flow; the
/// user's Continue resumes it, and it passes. Every kind of cut point is tried at least once.
#[test]
fn resume_after_cuts() {
    let mut w = World::new(2, 2, 3);
    w.pump();
    w.assert_passed("no cut");
    let cuts = w.cuts.clone();
    // The first point of each kind, and every 97th besides.
    let mut picked = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for (i, cut) in cuts.iter().enumerate() {
        if kinds.insert(format!("{cut:?}")) || i % 97 == 0 {
            picked.insert(i);
        }
    }
    let picked: Vec<usize> = picked.into_iter().collect();
    parallel(picked.len(), |n| {
        let at = picked[n];
        let mut w = World::new(2, 2, 3);
        w.cut_at = Some(at);
        w.pump();
        let cut = w.cut.clone().expect("the cut happened");
        w.assert_passed(&format!("cut {at}: {cut:?}"));
    });
}

/// Spec PK.7 (resume), binding (1): words 33–64 stay in the phone's RAM across an unplug, so
/// they aren't pulled again; an app restart loses them, and they're pulled again and checked
/// against the pinned h_C before anything else goes to the device.
#[test]
fn p_is_repulled_after_an_app_restart() {
    for app in [false, true] {
        let mut w = World::new(2, 2, 4);
        w.pump_until(|w| {
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::PhonePage { page: 3, .. }))
        });
        if app {
            w.cut_app(true);
        } else {
            w.cut_device(0);
        }
        let frames = w.frames.len();
        w.pump();
        w.assert_passed(&format!("app restart {app}"));
        // Served once per pull; a pull during the device's encode is `Busy` and sent again.
        let served = w
            .frames
            .iter()
            .filter(|f| matches!(f, Frame::ToPhone(DeviceCard::PhoneHalf(_))))
            .count();
        assert_eq!(served, if app { 2 } else { 1 }, "app restart {app}");
        if app {
            // The re-pull is the first request after the restart's Begin.
            let after: Vec<&str> = w.frames[frames..]
                .iter()
                .filter_map(|f| match f {
                    Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)) => Some(m.kind()),
                    _ => None,
                })
                .collect();
            assert_eq!(after.first(), Some(&"PullPhoneHalf"), "{after:?}");
        }
    }
}

/// Build plan A10, C7: words 33–64 are served from SEALED through CHECKING to a session, never
/// with a word of 1–32; they're refused once the card is PASSED or FAILED. The retired
/// `OpenPhoneWord` and `Prepare` are refused with `Unsupported`.
#[test]
fn pull_phone_half_refusals() {
    let mut w = World::new(2, 2, 5);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceHalf { .. })) && w.wire.is_empty());
    let h_c = w.record().unwrap().sealed.unwrap().h_c;
    for message in [
        CoordinatorCard::OpenPhoneWord { j: 40 },
        CoordinatorCard::OpenPhoneWord { j: 5 },
        CoordinatorCard::Prepare { h_c },
    ] {
        let replies = w.device_replies(message.clone());
        assert!(
            matches!(
                replies.as_slice(),
                [DeviceCard::Refused {
                    reason: CardRefusal::Unsupported
                }]
            ),
            "{message:?}: {replies:?}"
        );
    }
    // Words 33–64 go out only for the seal the phone names (audit-secrecy info).
    let mut other = h_c;
    other[0] ^= 1;
    let replies = w.device_replies(CoordinatorCard::PullPhoneHalf { h_c: other });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::BadRequest
            }]
        ),
        "{replies:?}"
    );
    let truth = w.truth;
    match w.pull_phone_half().as_slice() {
        [DeviceCard::PhoneHalf(words)] => assert_eq!(words.0[..], truth[PHONE_HALF]),
        other => panic!("{other:?}"),
    }
    w.pump();
    w.assert_passed("after the refusals");
    let replies = w.pull_phone_half();
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::Closed
            }]
        ),
        "{replies:?}"
    );
    w.session = None;
    let replies = w.pull_phone_half();
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::NoSession
            }]
        ),
        "{replies:?}"
    );
}

/// Build plan A10, A4: in CHECKING words 33–64 are still served (an app restart mid D check gets
/// them back), and the seal and the proofs are refused.
#[test]
fn checking_serves_p() {
    let mut w = World::new(2, 2, 6);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceRoundIntro)) && w.wire.is_empty());
    let replies = w.pull_phone_half();
    assert!(
        matches!(replies.as_slice(), [DeviceCard::PhoneHalf(_)]),
        "{replies:?}"
    );
    for message in [
        CoordinatorCard::PullCommit { from: 0, count: 1 },
        CoordinatorCard::PullProofs { from: 0, count: 1 },
    ] {
        let replies = w.device_replies(message.clone());
        assert!(
            matches!(
                replies.as_slice(),
                [DeviceCard::Refused {
                    reason: CardRefusal::WrongState
                }]
            ),
            "{message:?}: {replies:?}"
        );
    }
    w.pump();
    w.assert_passed("after the pulls");
}

/// The swap of words 33–64 a lying chip would serve.
fn swap_phone_half(frame: Frame) -> Tamper {
    match frame {
        Frame::ToPhone(DeviceCard::PhoneHalf(mut words)) => {
            words.0[7] = (words.0[7] + 1) % 2048;
            Tamper::Pass(Frame::ToPhone(DeviceCard::PhoneHalf(words)))
        }
        frame => Tamper::Pass(frame),
    }
}

/// Binding (1), spec PK.6: a lying chip can't swap words 33–64. P′ at the first pin is another
/// card: DifferentCard before any word is shown, and no `WriteStart`. P′ on the re-pull after an
/// app restart is a device fault: FAILED (PhoneHalfChanged) with the device suspect, before N;
/// after N it's an abort.
#[test]
fn a_lying_chip_cant_swap_the_phone_half() {
    // At the first pin.
    let mut w = World::new(2, 2, 7);
    w.filter = Some(Box::new(swap_phone_half));
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::DifferentCard)));
    assert!(!w.sent_to_device(|m| matches!(m, CoordinatorCard::WriteStart { .. })));
    assert!(w.device_seen.is_empty(), "no word of 1–32 was shown");
    assert!(w.card.iter().all(Option::is_none), "nothing was written");
    assert!(!w
        .phone
        .has(|m| matches!(m, CoordCardMutation::Pinned { .. })));

    // On the re-pull after an app restart, with the phone's half checked.
    let mut w = World::new(2, 2, 7);
    w.auto_final_check = false;
    w.pump();
    assert!(w.phone.checked);
    w.filter = Some(Box::new(swap_phone_half));
    w.cut_app(true);
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(
        matches!(
            w.events.last(),
            Some(CardEvent::Failed {
                reason: CardFailReason::PhoneHalfChanged,
                device_suspect: true,
                after_n: false
            })
        ),
        "{:?}",
        w.events.last()
    );
    assert_eq!(w.phone.pending_n, None, "no device round");

    // After N, once the device took it: an abort.
    let mut w = World::new(2, 2, 7);
    w.pump_until(|w| w.record().is_some_and(|rec| rec.n_tag.is_some()));
    w.filter = Some(Box::new(swap_phone_half));
    w.cut_app(true);
    w.pump();
    w.assert_aborted(CardFailReason::PhoneHalfChanged);
}

/// Owner B3: a different seal means a new card. With writing begun under the pinned h_C, the
/// device reports a seal other than the pinned one (its record's h_C is rewritten; h_C binds P
/// and the commit points, so this stands for a re-seal over another P): the phone fails the card
/// (SealChanged) on the device's status, and sends nothing after it but `Begin`; no word shows
/// again and the paper card is never continued under the new seal. An honest device can't show
/// another seal, so the device is marked suspect (owner B10), and again on a later flow that
/// meets the failed record. An honest re-show or app restart resumes the pinned seal and passes.
#[test]
fn a_device_reporting_another_seal_after_the_pin_fails() {
    let mut w = World::new(2, 2, 40);
    let flash = w.run.start_devices[&w.id].clone();
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::SelfCheck { .. })));
    let log = w
        .device_log
        .iter()
        .cloned()
        .map(|m| match m {
            DeviceCardMutation::SealedC {
                share_image,
                sealed,
                words_hash,
            } => DeviceCardMutation::SealedC {
                share_image,
                sealed: CardSealed {
                    h_c: [0x5E; 32],
                    ..sealed
                },
                words_hash,
            },
            m => m,
        })
        .collect::<Vec<_>>();
    w.replace_device(flash, log);
    let (frames, screens) = (w.frames.len(), w.screens.len());
    w.pump();
    assert!(
        matches!(
            w.events.last(),
            Some(CardEvent::Failed {
                reason: CardFailReason::SealChanged,
                device_suspect: true,
                after_n: false,
            })
        ),
        "{:?}",
        w.events.last()
    );
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect(), "owner B10: a changed seal is a suspect device");
    assert_eq!(
        w.run
            .coordinator
            .card_failure(w.asref, w.share_index)
            .map(|(reason, _)| reason),
        Some(CardFailReason::SealChanged)
    );
    // A later flow meets the failed record: the same alarm, nothing sent but `Begin`.
    let frames_before = w.frames.len();
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump();
    assert!(
        matches!(
            w.events.last(),
            Some(CardEvent::Failed {
                reason: CardFailReason::SealChanged,
                device_suspect: true,
                ..
            })
        ),
        "{:?}",
        w.events
    );
    assert!(w.frames[frames_before..]
        .iter()
        .all(|f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))));
    assert!(
        w.frames[frames..]
            .iter()
            .all(|f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))),
        "only Begin after the new seal"
    );
    assert!(!w.screens[screens..].iter().any(|s| matches!(
        s,
        CardScreen::DeviceHalf { .. } | CardScreen::SelfCheck { .. }
    )));
    // The phone's pin is the first seal, never replaced.
    let pins = w
        .phone
        .log
        .iter()
        .filter(|m| matches!(m, CoordCardMutation::Pinned { .. }))
        .count();
    assert_eq!(pins, 1);

    // An honest device: a re-show (unplug) and an app restart resume the pinned seal.
    let mut w = World::new(2, 2, 40);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::SelfCheck { .. })));
    w.cut_device(0);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::SelfCheck { .. })));
    w.cut_app(true);
    w.pump();
    w.assert_passed("re-show and restart");
    let pins = w
        .phone
        .log
        .iter()
        .filter(|m| matches!(m, CoordCardMutation::Pinned { .. }))
        .count();
    assert_eq!(pins, 1);
}

/// Design §3.11 #5: a chip that shows the user wrong words of 1–32 passes its own quiz (it
/// quizzes on what it showed), but the phone round opens the committed words of D: every one
/// is a miss, so the round fails after N and the device is marked suspect.
#[test]
fn a_lying_chip_is_caught_by_the_phone_round() {
    let mut w = World::new(2, 2, 8);
    let lookalikes: BTreeMap<u8, u16> = DEVICE_HALF
        .map(|j| (j as u8, lookalike_pool(w.truth[j]).unwrap()[0]))
        .collect();
    w.device_lies = lookalikes;
    w.pump();
    w.assert_aborted(CardFailReason::TwoMisses);
    w.assert_bound("a lying chip");
}

/// The catch rates design §5.4 quotes: a lie on `lied` words of 1–32 is among the phone round's
/// 5 with probability 1 − C(32 − lied, 5)/C(32, 5).
#[test]
fn phone_round_catch_rates() {
    let mut ops = Ops::default();
    for (lied, rate) in [(3u8, 0.41), (4, 0.51), (7, 0.74)] {
        let mut caught = 0;
        const DRAWS: u32 = 20_000;
        for seed in 0..DRAWS {
            let mut rng = StdRng::seed_from_u64(seed as u64);
            let mut n = [0u8; 32];
            rng.fill_bytes(&mut n);
            let d = proof::derive_boxes(&[3; 32], &n, &[4; 32], &mut ops);
            caught += d.iter().any(|&j| j < lied) as u32;
        }
        let measured = caught as f64 / DRAWS as f64;
        assert!((measured - rate).abs() < 0.02, "{lied}: {measured}");
    }
}

/// Owner B6, spec PK.11: D is drawn over all 32 device words, and every word of D is opened by
/// the carry opening: three frames, the word and its 77 Q's then 39 + 38 carry proofs, checked
/// against the proven seal and words 33–64 before its question. No other frame carries a word of
/// 1–32, and no word of D takes another path.
#[test]
fn every_d_word_is_a_carry_opening() {
    let mut ops = Ops::default();
    let mut hits = [0u32; HALF];
    for seed in 0..10_000u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut n = [0u8; 32];
        rng.fill_bytes(&mut n);
        for j in proof::derive_boxes(&[3; 32], &n, &[4; 32], &mut ops) {
            hits[j as usize] += 1;
        }
    }
    // 10,000 × 5 / 32 ≈ 1,562 each.
    assert!(
        hits.iter().all(|&h| (1_350..1_780).contains(&h)),
        "{hits:?}"
    );

    let mut w = World::new(2, 2, 46);
    w.pump();
    w.assert_passed("carry openings");
    let d = w.record().unwrap().d.unwrap();
    let parts: Vec<&CardOpeningPart> = w
        .frames
        .iter()
        .filter_map(|f| match f {
            Frame::ToPhone(DeviceCard::OpeningPart(part)) => Some(part),
            _ => None,
        })
        .collect();
    let part_j = |part: &CardOpeningPart| match part {
        CardOpeningPart::Carries { j, .. } | CardOpeningPart::Proofs { j, .. } => *j,
    };
    assert!(parts.iter().all(|part| d.contains(&part_j(part))));
    for &j in &d {
        let mine: Vec<&&CardOpeningPart> = parts.iter().filter(|part| part_j(part) == j).collect();
        match mine.as_slice() {
            [CardOpeningPart::Carries { carries, .. }, CardOpeningPart::Proofs {
                first: 0,
                proofs: first,
                ..
            }, CardOpeningPart::Proofs {
                first: 39,
                proofs: second,
                ..
            }] => {
                assert_eq!(carries.len(), 77);
                assert_eq!((first.len(), second.len()), (39, 38));
            }
            other => panic!("word {}: {other:?}", j + 1),
        }
    }
}

/// Design §3.11 #6: a phone that shows wrong words of 33–64 passes its own quiz, but the
/// device round asks 5 of them it drew itself: a miss, a second look, and a second miss fail
/// the card. Owner B8: the chip knew D by then, so the device is suspect too (a lying chip
/// can't fail the round to have the phone blamed and the card drawn again).
#[test]
fn a_lying_phone_is_caught_by_the_device_round() {
    let mut w = World::new(2, 2, 9);
    w.phone_lies = PHONE_HALF
        .map(|j| (j as u8, lookalike_pool(w.truth[j]).unwrap()[0]))
        .collect();
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert_eq!(
        w.state(),
        Some(CardState::Failed(VerdictReason::DeviceRoundFailed))
    );
    assert!(
        w.events.iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                reason: CardFailReason::DeviceRoundFailed,
                device_suspect: true,
                after_n: true,
            }
        )),
        "{:?}",
        w.events.last()
    );
    // Owner B8: N went out with the confirm, before the device round.
    assert!(w.phone.n.is_some());
    assert!(w.sent_to_device(|m| matches!(m, CoordinatorCard::OpenStart { .. })));
    w.assert_bound("a lying phone");
}

/// Design §3.11 #9, #11, #10: a later check from the same phone (h37) gets words 33–64 and D's
/// words again, quizzes 37 items on each screen, persists nothing and draws nothing; a phone
/// with no record gets no word (`NoRecord`) and no "Backup checked" while the device still
/// quizzes its own 37; a wrong hash gets none either. Across all of it each screen saw the same 37 words.
#[test]
fn rechecks() {
    let mut w = World::new(2, 2, 10);
    let mut stranger = Phone::new(w.run.coordinator.clone());
    w.pump();
    w.assert_passed("backup");
    let seen = (w.device_seen.clone(), w.phone_seen.clone());
    let device_log = w.device_log.len();
    let phone_log = w.phone.log.len();

    // The same phone.
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump();
    assert!(w.sent_to_device(|m| matches!(
        m,
        CoordinatorCard::Recheck {
            words_hash: Some(_)
        }
    )));
    assert!(w.sent_to_phone(|m| matches!(
        m,
        DeviceCard::RecheckWords {
            d_words: Some(_),
            ..
        }
    )));
    let questions = w
        .events
        .iter()
        .filter(|e| matches!(e, CardEvent::SelfCheckQuestion { recheck: true, .. }))
        .count();
    assert_eq!(questions, HALF + CHECK_BOXES);
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckPassed)));
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckDeviceDone)));
    assert!(matches!(w.screen, Some(CardScreen::RecheckDone)));
    assert_eq!(
        w.device_log.len(),
        device_log,
        "nothing persisted on the device"
    );
    assert_eq!(
        w.phone.log.len(),
        phone_log,
        "nothing persisted on the phone"
    );
    assert_eq!((w.device_seen.clone(), w.phone_seen.clone()), seen);

    // A phone with no record of the card.
    w.swap_phone(&mut stranger);
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Begin);
    let frames = w.frames.len();
    w.pump();
    assert!(w.sent_to_device(|m| matches!(m, CoordinatorCard::Recheck { words_hash: None })));
    assert!(!w.frames[frames..]
        .iter()
        .any(|f| matches!(f, Frame::ToPhone(DeviceCard::RecheckWords { .. }))));
    assert!(w.frames[frames..].iter().any(|f| matches!(
        f,
        Frame::ToPhone(DeviceCard::Refused {
            reason: CardRefusal::NoRecord
        })
    )));
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckUnavailable)));
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckDeviceDone)));
    // The device's half is checked, but nothing here says "Backup checked" (audit F1).
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
    assert_eq!(w.status(), CardBackupStatus::NoCard);
    assert_eq!((w.device_seen.clone(), w.phone_seen.clone()), seen);

    // A wrong hash: no word, and the device's quiz again.
    w.swap_phone(&mut stranger);
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::Recheck { .. })));
    w.pump();
    let replies = w.device_replies(CoordinatorCard::Recheck {
        words_hash: Some([9; 32]),
    });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::NoRecord
            }]
        ),
        "{replies:?}"
    );
    assert!(matches!(w.screen, Some(CardScreen::Recheck { .. })));
    assert_eq!((w.device_seen.clone(), w.phone_seen.clone()), seen);
    w.run.check_mutations();
}

/// Owner B9, one phone per check (design §3.11 #19, #11 as amended): the check runs only on the
/// phone that started the backup, which pinned the seal before any word was shown. A second
/// phone, with no record, that meets the device anywhere past SEALED (words 1–32 on its screen,
/// its quiz, its half checked before N, or the first phone's check after N) gets
/// `StartedOnAnotherPhone`: it records nothing, sends nothing but `Begin`, never draws N and so
/// can never reach Checked, and nobody is suspect. The first phone then finishes the card. Once
/// it passed, the second phone's later check is only the device's half: still not Checked.
#[test]
fn second_phone() {
    type Point = fn(&mut World) -> bool;
    let points: [(&str, Point); 4] = [
        ("words 1–32 on the device", |w| {
            matches!(w.screen, Some(CardScreen::DeviceHalf { .. }))
        }),
        ("the device's quiz", |w| {
            matches!(w.screen, Some(CardScreen::SelfCheck { .. }))
        }),
        ("its half checked, before N", |w| {
            matches!(w.screen, Some(CardScreen::PhoneHalfNext))
        }),
        ("after the first phone's N", |w| {
            w.record().is_some_and(|rec| rec.n_tag.is_some())
        }),
    ];
    for (i, (point, reached)) in points.into_iter().enumerate() {
        let mut w = World::new(2, 2, 11 + 100 * i as u64);
        let mut second = Phone::new(w.run.coordinator.clone());
        w.auto_confirm = !point.contains("before N");
        w.pump_until(reached);
        let state = w.state();
        assert!(!matches!(state, Some(CardState::Sealed)), "{point}");
        // What the first phone's user was about to do stays with the first phone.
        w.phone_actions.clear();

        // The second phone: the device is moved to it, so it boots fresh.
        w.swap_phone(&mut second);
        w.events.clear();
        let frames = w.frames.len();
        w.cut_device(0);
        w.pump_until(|w| {
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::StartedOnAnotherPhone))
        });
        w.pump();
        assert!(w.phone.log.is_empty(), "{point}: nothing recorded");
        assert_eq!(w.status(), CardBackupStatus::NoCard, "{point}");
        assert!(
            w.frames[frames..].iter().all(
                |f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))
            ),
            "{point}: only Begin"
        );
        assert!(
            !w.suspect(),
            "{point}: an honest second phone: no suspect mark"
        );
        assert_eq!(w.state(), state, "{point}: the device is where it was");

        // The first phone finishes.
        w.swap_phone(&mut second);
        w.auto_confirm = true;
        w.cut_device(0);
        w.pump();
        w.errors.clear();
        w.assert_passed(&format!("first phone, {point}"));

        // A later check from the second phone: the device's half only, never Checked.
        w.swap_phone(&mut second);
        w.events.clear();
        w.cut_device(0);
        w.pump();
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::RecheckUnavailable)),
            "{point}: {:?}",
            w.events
        );
        assert!(!w
            .events
            .iter()
            .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
        assert_eq!(w.status(), CardBackupStatus::NoCard, "{point}");
        assert!(w.phone.log.is_empty(), "{point}");
    }
}

/// Owner B15: an app install that holds the wallet but didn't run its keygen may still write the
/// card, words 1–32 on the device and 33–64 on the phone, with both quizzes, once the user chose
/// to on its intro. It never pins, draws N, pulls proofs or openings, and each screen sees
/// exactly its own 32 words. Its record is "written, not verified" (`Unverified`), never
/// Checked, and the card restores. The keygen's install then meets a device past SEALED with no
/// pin of its own: another phone's backup, recorded nowhere. Opened again, the other install
/// shows its written card and sends nothing but `Begin`.
#[test]
fn another_install_writes_the_card_unverified() {
    let mut w = World::on_other_install(2, 2, 15);
    w.auto_write_unverified = false;
    w.pump();
    assert!(matches!(w.events.last(), Some(CardEvent::UnverifiedIntro)));
    assert_eq!(
        w.state(),
        Some(CardState::Sealed),
        "nothing before the choice"
    );
    assert!(w.phone.log.is_empty());
    assert!(!w.sent_to_device(|m| !matches!(m, CoordinatorCard::Begin(_))));
    assert_eq!(w.status(), CardBackupStatus::NoCard);

    w.phone_actions.push_back(PhoneAction::WriteUnverified);
    w.pump();
    w.assert_unverified("unverified");
    assert!(w
        .run
        .coordinator
        .card_final_begin(w.id, &mut StdRng::seed_from_u64(1))
        .is_err());
    // The card restores: both halves written as the share's.
    let target = codec::Target::Share {
        image: w.share_image,
        threshold: 2,
    };
    assert!(codec::decode(&w.card, Some(&target), None).is_ok());

    // Opened again on the same install: its card is written, and nothing goes to the device.
    let frames = w.frames.len();
    w.events.clear();
    w.cut_device(0);
    w.pump();
    assert!(matches!(
        w.events.last(),
        Some(CardEvent::UnverifiedWritten)
    ));
    assert!(w.frames[frames..]
        .iter()
        .all(|f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))));

    // The keygen's install: the device is past SEALED and it has no pin.
    let mut keygen_phone = Phone::new(key(2, 2, 15).start_coordinator.clone());
    w.swap_phone(&mut keygen_phone);
    assert!(w.created_here());
    w.events.clear();
    w.cut_device(0);
    w.pump();
    assert!(
        matches!(w.events.last(), Some(CardEvent::StartedOnAnotherPhone)),
        "{:?}",
        w.events
    );
    assert!(w.phone.log.is_empty());
    assert_eq!(w.status(), CardBackupStatus::NoCard);
    assert_eq!(w.state(), Some(CardState::CardChecked));
}

/// Review B15 protocol F4: another install recorded its unverified start, but its `WriteStart`
/// was lost, so the device stayed SEALED, and the user then wrote and checked the card on the
/// install that ran the keygen. Back on the other install the device reports PASSED under the
/// seal it started: its record becomes "written, not verified" rather than "not written yet"
/// for good, and nothing else changes: no pin, no N, never Checked, the device's half alone in
/// the later check.
#[test]
fn an_unverified_start_the_keygen_install_finished_is_written_not_verified() {
    let mut w = World::on_other_install(2, 2, 18);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToDevice(CoordinatorCard::WriteStart { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    w.filter = None;
    assert_eq!(
        w.state(),
        Some(CardState::Sealed),
        "the WriteStart was lost"
    );
    assert_eq!(w.status(), CardBackupStatus::NotChecked);
    assert!(w
        .phone
        .log
        .iter()
        .any(|m| matches!(m, CoordCardMutation::UnverifiedWriteStarted { .. })));
    assert!(w.phone_half_in_ram(), "the start pulled the phone half");

    let mut keygen_phone = Phone::new(key(2, 2, 18).start_coordinator.clone());
    w.swap_phone(&mut keygen_phone);
    w.cut_device(0);
    w.pump();
    w.errors.clear();
    w.assert_passed("the keygen install");

    w.swap_phone(&mut keygen_phone);
    for opened in ["first", "again"] {
        w.events.clear();
        let frames = w.frames.len();
        w.cut_device(0);
        w.pump();
        assert_eq!(w.status(), CardBackupStatus::Unverified, "{opened}");
        assert_eq!(
            w.phone
                .log
                .iter()
                .filter(|m| matches!(m, CoordCardMutation::UnverifiedWritten { .. }))
                .count(),
            1,
            "{opened}: {:?}",
            w.phone.log
        );
        assert!(
            w.phone.log.iter().all(|m| matches!(
                m,
                CoordCardMutation::UnverifiedWriteStarted { .. }
                    | CoordCardMutation::UnverifiedWritten { .. }
            )),
            "{opened}: {:?}",
            w.phone.log
        );
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::RecheckUnavailable)),
            "{opened}: {:?}",
            w.events
        );
        assert!(!w
            .events
            .iter()
            .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
        assert!(
            w.frames[frames..].iter().all(|f| !matches!(
                f,
                Frame::ToDevice(m)
                    if !matches!(m, CoordinatorCard::Begin(_) | CoordinatorCard::Recheck { .. })
            )),
            "{opened}: only Begin and the device's half of a later check"
        );
        assert!(!w.suspect(), "{opened}");
        assert_eq!(w.state(), Some(CardState::Passed), "{opened}");
        assert!(!w.phone_half_in_ram(), "{opened}: the words left RAM");
    }
}

/// Owner B15: the unverified card resumes like the verified one before N, at every cut: an
/// unplug or an app restart while either half is written or quizzed, a lost frame, or a persist
/// the app or the device didn't finish. It ends written, not verified, with each screen's 32
/// words, and nothing of the verified check is ever sent or recorded.
#[test]
fn an_unverified_card_resumes_after_cuts() {
    let mut w = World::on_other_install(2, 2, 16);
    w.pump();
    w.assert_unverified("no cut");
    let cuts = w.cuts.clone();
    parallel(cuts.len(), |at| {
        let mut w = World::on_other_install(2, 2, 16);
        w.cut_at = Some(at);
        w.pump();
        let cut = w.cut.clone().expect("the cut happened");
        w.assert_unverified(&format!("cut {at}: {cut:?}"));
    });
}

/// Owner B15, with the later-check and one-phone rules (owner B9, PK-O7): an install that didn't
/// run the keygen meets the keygen install's card at each point past SEALED. While it's written
/// or checked there, it's another phone's backup (`StartedOnAnotherPhone`): nothing recorded,
/// nothing sent but `Begin`. Once it passed, the later check is the device's half only, as from
/// any phone with no record, and never Checked.
#[test]
fn another_install_meets_the_keygen_installs_card() {
    type Point = fn(&mut World) -> bool;
    let points: [(&str, Point); 3] = [
        ("words 1–32 on the device", |w| {
            matches!(w.screen, Some(CardScreen::DeviceHalf { .. }))
        }),
        ("its half checked, before N", |w| {
            matches!(w.screen, Some(CardScreen::PhoneHalfNext))
        }),
        ("after N", |w| {
            w.record().is_some_and(|rec| rec.n_tag.is_some())
        }),
    ];
    for (i, (point, reached)) in points.into_iter().enumerate() {
        let seed = 17 + 100 * i as u64;
        let mut w = World::new(2, 2, seed);
        let mut other = other_install(2, 2, seed);
        w.auto_confirm = !point.contains("before N");
        w.pump_until(reached);
        let state = w.state();
        w.phone_actions.clear();
        w.swap_phone(&mut other);
        w.events.clear();
        let frames = w.frames.len();
        w.cut_device(0);
        w.pump();
        assert!(
            matches!(w.events.last(), Some(CardEvent::StartedOnAnotherPhone)),
            "{point}: {:?}",
            w.events
        );
        assert!(w.phone.log.is_empty(), "{point}");
        assert_eq!(w.status(), CardBackupStatus::NoCard, "{point}");
        assert!(
            w.frames[frames..].iter().all(
                |f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))
            ),
            "{point}: only Begin"
        );
        assert!(!w.suspect(), "{point}");
        assert_eq!(w.state(), state, "{point}");

        // The keygen install finishes; the other install's later check is the device's half.
        w.swap_phone(&mut other);
        w.auto_confirm = true;
        w.cut_device(0);
        w.pump();
        w.errors.clear();
        w.assert_passed(&format!("keygen install, {point}"));
        w.swap_phone(&mut other);
        w.events.clear();
        w.cut_device(0);
        w.pump();
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::RecheckUnavailable)),
            "{point}: {:?}",
            w.events
        );
        assert!(!w
            .events
            .iter()
            .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
        assert_eq!(w.status(), CardBackupStatus::NoCard, "{point}");
        assert!(w.phone.log.is_empty(), "{point}");
    }
}

/// Spec PK.7: `FinalBegin` is accepted only from PROVING; in CARD_CHECKED it's refused and
/// nothing is drawn. The phone's [Continue] needs both halves recorded and every proof verified.
#[test]
fn final_begin_needs_proving() {
    let mut w = World::new(2, 2, 12);
    w.auto_confirm = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::CardChecked));
    let replies = w.device_replies(CoordinatorCard::FinalBegin);
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::WrongState
            }]
        ),
        "{replies:?}"
    );
    assert!(w.record().unwrap().device_round.is_none(), "E isn't drawn");
    let mut rng = StdRng::seed_from_u64(1);
    assert!(w.run.coordinator.card_final_begin(w.id, &mut rng).is_err());

    // Confirmed, and the phone's half checked, but the proofs not all verified: no [Continue].
    w.auto_confirm = true;
    w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
    w.pump_until(|w| w.phone.checked);
    assert!(!w.phone.proven);
    assert!(w.run.coordinator.card_final_begin(w.id, &mut rng).is_err());
    assert!(w.run.coordinator.staged_mutations().is_empty());
    w.pump();
    w.assert_passed("after the refusals");
}

/// Owner B1, build plan A4: `PullProofs` is `Busy` in SEALED through CARD_CHECKED and arms no
/// job, and `DeviceHalfWritten` is refused before CARD_CHECKED (`WrongState`) or with another h_C
/// (`BadRequest`) and remembers nothing: no proof is armed until the device's quiz passed AND the
/// confirm came. Then the device enters PROVING and the proofs are served.
#[test]
fn proofs_only_after_device_half_written() {
    let mut w = World::new(2, 2, 42);
    let busy_and_idle = |w: &mut World, context: &str| {
        let h_c = w.record().unwrap().sealed.unwrap().h_c;
        let state = w.state();
        let replies = w.device_replies(CoordinatorCard::PullProofs { from: 0, count: 16 });
        assert!(
            matches!(
                replies.as_slice(),
                [DeviceCard::Refused {
                    reason: CardRefusal::Busy
                }]
            ),
            "{context}: {replies:?}"
        );
        if state != Some(CardState::CardChecked) {
            let replies = w.device_replies(CoordinatorCard::DeviceHalfWritten { h_c });
            assert!(
                matches!(
                    replies.as_slice(),
                    [DeviceCard::Refused {
                        reason: CardRefusal::WrongState
                    }]
                ),
                "{context}: {replies:?}"
            );
        }
        let replies = w.device_replies(CoordinatorCard::DeviceHalfWritten { h_c: [7; 32] });
        assert!(
            matches!(
                replies.as_slice(),
                [DeviceCard::Refused {
                    reason: CardRefusal::BadRequest
                }]
            ),
            "{context}: {replies:?}"
        );
        assert_eq!(w.state(), state, "{context}: nothing is remembered");
        assert!(
            w.session.as_ref().unwrap().progress().is_none(),
            "{context}: no job is armed"
        );
    };
    // SEALED: the phone's `WriteStart` is held back.
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToDevice(CoordinatorCard::WriteStart { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(w.state(), Some(CardState::Sealed));
    busy_and_idle(&mut w, "SEALED");
    // WRITING and SELF_CHECK, with the user at the screen.
    w.filter = None;
    w.device_holds = Some(|s| matches!(s, CardScreen::DeviceHalf { .. }));
    w.auto_confirm = false;
    w.cut_device(0);
    w.pump();
    assert_eq!(w.state(), Some(CardState::Writing));
    busy_and_idle(&mut w, "WRITING");
    w.device_holds = Some(|s| matches!(s, CardScreen::SelfCheck { .. }));
    w.acted = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::SelfCheck));
    busy_and_idle(&mut w, "SELF_CHECK");
    // CARD_CHECKED: the quiz passed, but no confirm yet.
    w.device_holds = None;
    w.acted = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::CardChecked));
    busy_and_idle(&mut w, "CARD_CHECKED");
    // The confirm: PROVING, and the proofs are armed.
    let h_c = w.record().unwrap().sealed.unwrap().h_c;
    let replies = w.device_replies(CoordinatorCard::DeviceHalfWritten { h_c });
    assert!(replies.is_empty(), "{replies:?}");
    assert_eq!(w.state(), Some(CardState::Proving));
    assert!(w.session.as_ref().unwrap().progress().is_some());
    w.auto_confirm = true;
    w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
    w.pump();
    w.assert_passed("after the confirm");
}

/// Owner B1: `DeviceHalfWritten` again in PROVING changes nothing and restarts nothing.
#[test]
fn device_half_written_is_idempotent() {
    let mut w = World::new(2, 2, 48);
    w.phone_holds = Some(|e| matches!(e, CardEvent::PhonePage { page: 0, .. }));
    w.pump_until(|w| w.state() == Some(CardState::Proving) && w.wire.is_empty());
    let h_c = w.record().unwrap().sealed.unwrap().h_c;
    let progress = w.session.as_ref().unwrap().progress();
    let log = w.device_log.len();
    let replies = w.device_replies(CoordinatorCard::DeviceHalfWritten { h_c });
    assert!(replies.is_empty(), "{replies:?}");
    assert_eq!(w.device_log.len(), log, "nothing persisted");
    assert_eq!(w.session.as_ref().unwrap().progress(), progress);
    w.phone_holds = None;
    w.phone_actions.push_back(PhoneAction::Page(1));
    w.pump();
    w.assert_passed("after the repeat");
}

/// Spec PK.7 timing rule: from `WriteStart` to the confirm no seal, proof or opening job runs on
/// the device; a new session in WRITING, SELF_CHECK or CARD_CHECKED (an unplug) runs its encode
/// and nothing else, and the seal before word 1 is the only one until PROVING. (`World::step_job`
/// checks the rule on every job of every test.)
#[test]
fn no_job_runs_while_writing() {
    let screens: [fn(&CardScreen) -> bool; 2] = [
        |s| matches!(s, CardScreen::DeviceHalf { .. }),
        |s| matches!(s, CardScreen::SelfCheck { .. }),
    ];
    for hold in screens.into_iter().map(Some).chain([None]) {
        let mut w = World::new(2, 2, 41);
        w.device_holds = hold;
        w.auto_confirm = false;
        w.pump();
        assert_eq!(
            w.job_events,
            ["Encoded", "Sealed"],
            "the seal before word 1"
        );
        w.cut_device(0);
        w.job_events.clear();
        w.pump();
        let state = w.state();
        assert_eq!(w.job_events, ["Encoded"], "{state:?}");
        assert!(w.session.as_ref().unwrap().progress().is_none());
        // The phone asks for the confirm once the device's quiz passed; in CARD_CHECKED it has.
        if hold.is_none() {
            w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
        }
        w.device_holds = None;
        w.acted = false;
        w.auto_confirm = true;
        w.pump();
        w.assert_passed("after the unplug");
    }
}

/// Spec PK.7, build plan A4: the seal is computed before word 1 and for the proofs only. A new
/// session in WRITING seals nothing, and a `PullCommit` there is refused rather than start the
/// seal. In PROVING the seal is computed again for the proofs, and then served.
#[test]
fn the_seal_on_demand() {
    let mut w = World::new(2, 2, 13);
    w.device_holds = Some(|s| matches!(s, CardScreen::DeviceHalf { .. }));
    w.pump();
    w.cut_device(0);
    w.job_events.clear();
    w.pump();
    assert_eq!(w.job_events, ["Encoded"]);
    let replies = w.device_replies(CoordinatorCard::PullCommit { from: 0, count: 1 });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::WrongState
            }]
        ),
        "{replies:?}"
    );
    assert!(w.session.as_ref().unwrap().progress().is_none());

    // PROVING after an unplug: the seal again, for the proofs, and then served.
    w.device_holds = None;
    w.acted = false;
    w.auto_confirm = false;
    w.pump();
    w.cut_device(0);
    w.job_events.clear();
    w.pump();
    assert_eq!(w.job_events, ["Encoded"], "CARD_CHECKED seals nothing");
    w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
    w.pump_until(|w| w.job_events.contains(&"Sealed") && w.wire.is_empty());
    assert_eq!(w.job_events, ["Encoded", "Sealed"]);
    let served = w.device_replies(CoordinatorCard::PullCommit { from: 0, count: 1 });
    assert!(
        matches!(served.as_slice(), [DeviceCard::CommitPoints { .. }]),
        "{served:?}"
    );
    w.auto_confirm = true;
    w.pump();
    w.assert_passed("after the pulls");
}

/// Hardening (S3 review): a `WriteStart` in SEALED stops a seal job that a pull in the same
/// session armed, so no seal job runs in WRITING whatever the phone sends.
#[test]
fn write_start_stops_a_seal_a_pull_armed() {
    let mut w = World::new(2, 2, 42);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToDevice(CoordinatorCard::WriteStart { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(w.state(), Some(CardState::Sealed));
    w.cut_device(0);
    w.job_events.clear();
    w.pump();
    assert_eq!(w.state(), Some(CardState::Sealed));
    assert_eq!(w.job_events, ["Encoded"]);
    w.filter = None;
    let replies = w.device_replies(CoordinatorCard::PullCommit { from: 0, count: 1 });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::Busy
            }]
        ),
        "{replies:?}"
    );
    let task = |w: &World| w.session.as_ref().unwrap().progress().map(|p| p.task);
    assert_eq!(task(&w), Some(BusyTask::Seal));
    let h_c = w.record().unwrap().sealed.unwrap().h_c;
    w.device_replies(CoordinatorCard::WriteStart { h_c });
    assert_eq!(w.state(), Some(CardState::Writing));
    assert_eq!(task(&w), None, "the seal job is stopped");
    w.cut_device(0);
    w.pump();
    w.assert_passed("after the stopped seal");
}

/// Owner B8: the device round runs only once N is taken. The transition rule refuses a
/// `DeviceRoundStarted` in PROVING with no N (a replayed log can't skip N either), takes N there,
/// and only then the device round.
#[test]
fn the_device_round_needs_n_first() {
    let share_image = World::new(2, 2, 16).share_image;
    let mut device =
        FrostSigner::new_random_with_nonce_batch_size(&mut StdRng::seed_from_u64(1), 8, 10);
    let mut apply =
        |m: DeviceCardMutation| device.apply_mutation(DeviceMutation::Card(m)).is_some();
    for m in [
        DeviceCardMutation::Origin {
            share_image,
            origin: ShareOrigin::Keygen { card: true },
        },
        DeviceCardMutation::SealedC {
            share_image,
            sealed: CardSealed {
                format_id: proof::FORMAT_ID,
                h_c: [1; 32],
                coin_commit: [2; 32],
            },
            words_hash: [3; 32],
        },
        DeviceCardMutation::SplitWriteStarted { share_image },
        DeviceCardMutation::DeviceHalfWritten { share_image },
        DeviceCardMutation::CardChecked { share_image },
        DeviceCardMutation::ProvingStarted { share_image },
    ] {
        assert!(apply(m.clone()), "{m:?}");
    }
    let round = DeviceCardMutation::DeviceRoundStarted {
        share_image,
        e: [40, 41, 42, 43, 44],
    };
    assert!(!apply(round.clone()), "no device round before N");
    assert!(apply(DeviceCardMutation::PhoneRound {
        share_image,
        n_tag: [4; 32],
        d: [1, 2, 3, 4, 5],
    }));
    assert!(apply(round), "the device round after N");
    let rec = device.card_record(&share_image).unwrap();
    assert_eq!(rec.state, CardState::Checking);
    assert_eq!(rec.n_tag, Some([4; 32]));
}

/// Design §3.11 #17: one card per share. A restored share is refused; a second `SealedC`, and
/// a `DeviceRoundStarted`, `PhoneRound`, `SplitWriteStarted` or `ProvingStarted` out of turn, are
/// refused; and a replayed `SealedC` of another card format makes the record `OtherFormat`, which
/// refuses every request with `Unsupported`.
#[test]
fn one_card_per_share() {
    let mut w = World::new(2, 2, 15);
    w.pump();
    w.assert_passed("backup");
    let share_image = w.share_image;
    let rec = w.record().unwrap();
    let device = w.device();
    let again = [
        DeviceCardMutation::SealedC {
            share_image,
            sealed: CardSealed {
                format_id: proof::FORMAT_ID,
                h_c: [1; 32],
                coin_commit: [2; 32],
            },
            words_hash: [3; 32],
        },
        DeviceCardMutation::DeviceRoundStarted {
            share_image,
            e: [40, 41, 42, 43, 44],
        },
        DeviceCardMutation::PhoneRound {
            share_image,
            n_tag: [3; 32],
            d: [1, 2, 3, 4, 5],
        },
        DeviceCardMutation::SplitWriteStarted { share_image },
        DeviceCardMutation::ProvingStarted { share_image },
    ];
    for m in again {
        assert!(
            device
                .apply_mutation(DeviceMutation::Card(m.clone()))
                .is_none(),
            "{m:?}"
        );
    }
    assert_eq!(device.card_record(&share_image), Some(&rec));

    // A record sealed in a card format this firmware doesn't write.
    let mut old =
        FrostSigner::new_random_with_nonce_batch_size(&mut StdRng::seed_from_u64(1), 8, 10);
    old.apply_mutation(DeviceMutation::Card(DeviceCardMutation::Origin {
        share_image,
        origin: ShareOrigin::Keygen { card: true },
    }));
    old.apply_mutation(DeviceMutation::Card(DeviceCardMutation::SealedC {
        share_image,
        sealed: CardSealed {
            format_id: proof::FORMAT_ID + 1,
            h_c: [1; 32],
            coin_commit: [2; 32],
        },
        words_hash: [3; 32],
    }));
    assert_eq!(
        old.card_record(&share_image).unwrap().state,
        CardState::OtherFormat
    );
    assert!(old
        .apply_mutation(DeviceMutation::Card(
            DeviceCardMutation::SplitWriteStarted { share_image }
        ))
        .is_none());
    assert_eq!(old.card_home_line(share_image), None);
    let other_format = old.card_record(&share_image).cloned();
    for out in [
        CardOutput::Status,
        CardOutput::CheckDone,
        CardOutput::DeviceHalfScreen,
        CardOutput::PhoneHalf,
    ] {
        assert_eq!(
            card_gate(other_format.as_ref(), None, &out, &mut Ops::default()),
            Err(CardRefusal::Unsupported)
        );
    }
}

/// Design §3.11 #18 with owner B1: where a resume lands, by the device's state and the phone's
/// record and RAM.
#[test]
fn the_resume_table() {
    // Device in CARD_CHECKED, no confirm yet: the confirm is offered again, and nothing is
    // pulled but words 33–64 are kept.
    let mut w = World::new(2, 2, 17);
    w.auto_confirm = false;
    w.pump();
    w.cut_device(0);
    w.events.clear();
    let frames = w.frames.len();
    w.pump();
    assert!(
        matches!(w.events.last(), Some(CardEvent::ConfirmDeviceHalf)),
        "{:?}",
        w.events
    );
    assert!(!w.frames[frames..]
        .iter()
        .any(|f| matches!(f, Frame::ToDevice(CoordinatorCard::PullProofs { .. }))));
    w.auto_confirm = true;
    w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
    w.pump();
    w.assert_passed("from the confirm");

    // The confirm persisted but lost on its way: the device is still in CARD_CHECKED and answers
    // the pull `Busy`, so the phone sends the confirm again, with no unplug.
    let lose_the_confirm = || {
        let mut dropped = false;
        Box::new(move |frame| match frame {
            Frame::ToDevice(CoordinatorCard::DeviceHalfWritten { .. }) if !dropped => {
                dropped = true;
                Tamper::Drop
            }
            frame => Tamper::Pass(frame),
        }) as Filter
    };
    let mut w = World::new(2, 2, 18);
    w.filter = Some(lose_the_confirm());
    w.pump();
    w.assert_passed("the confirm again on Busy");
    // The lost one isn't in `frames`: the one delivered is the re-send.
    assert_eq!(
        w.count_to_device(|m| matches!(m, CoordinatorCard::DeviceHalfWritten { .. })),
        1
    );

    // The same, with an unplug before the pull: the resume sends the confirm again.
    let mut w = World::new(2, 2, 18);
    w.filter = Some(lose_the_confirm());
    w.pump_until(|w| w.phone.device_half_written);
    assert_eq!(w.state(), Some(CardState::CardChecked));
    w.cut_device(0);
    w.pump();
    w.assert_passed("the confirm again");

    // Device in PROVING, the phone on its pages: an unplug lands on the same page, and the
    // proofs go on. The device's new session shows no word of 1–32 again (owner B2).
    let mut w = World::new(2, 2, 19);
    w.phone_holds = Some(|e| matches!(e, CardEvent::PhonePage { page: 4, .. }));
    w.pump();
    w.cut_device(0);
    let screens = w.screens.len();
    w.events.clear();
    w.pump_until(|w| {
        w.events
            .iter()
            .any(|e| matches!(e, CardEvent::PhonePage { .. }))
    });
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::PhonePage { page: 4, .. })));
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::Proving { .. } | CardEvent::Proven)));
    w.pump_until(|w| w.session.as_ref().is_some_and(|s| s.progress().is_none()));
    assert!(
        w.screens[screens..].iter().all(|s| matches!(
            s,
            CardScreen::Preparing { .. } | CardScreen::CardFinishedWaiting
        )),
        "{:?}",
        &w.screens[screens..]
    );
    let h_c = w.record().unwrap().sealed.unwrap().h_c;
    let replies = w.device_replies(CoordinatorCard::WriteStart { h_c });
    assert!(replies.is_empty(), "{replies:?}");
    assert!(!w.screens[screens..].iter().any(|s| matches!(
        s,
        CardScreen::DeviceHalf { .. } | CardScreen::SelfCheck { .. }
    )));
    w.phone_holds = None;
    w.phone_actions.push_back(PhoneAction::Page(5));
    w.pump();
    w.assert_passed("from the pages");

    // Device in PROVING, the phone's half checked, an app restart: words 33–64 are pulled again,
    // then the missing proofs, then the final check.
    let mut w = World::new(2, 2, 20);
    w.auto_final_check = false;
    w.pump_until(|w| w.phone.checked);
    w.cut_app(true);
    w.events.clear();
    w.auto_final_check = true;
    w.pump();
    w.assert_passed("from an app restart");

    // Device in PROVING, both halves recorded and proven: the final check is next.
    let mut w = World::new(2, 2, 21);
    w.auto_final_check = false;
    w.pump();
    w.cut_device(0);
    w.events.clear();
    w.pump();
    assert!(matches!(
        w.events.last(),
        Some(CardEvent::ReadyForFinalCheck)
    ));
    w.auto_final_check = true;
    w.phone_actions.push_back(PhoneAction::FinalCheck);
    w.pump();
    w.assert_passed("from the final check");

    // core-protocol F1: [Continue] taken (the round persisted as pending) but `FinalBegin`
    // lost before the device took it, to an unplug, an app restart, or a frame dropped with the
    // device plugged in. The device is still proving, and the phone sends `FinalBegin` again on
    // the same N by itself: no second [Continue], and no step back to it.
    for cut in ["unplug", "app restart", "dropped frame"] {
        let mut w = World::new(2, 2, 23);
        if cut == "dropped frame" {
            let mut dropped = false;
            w.filter = Some(Box::new(move |frame| match frame {
                Frame::ToDevice(CoordinatorCard::FinalBegin) if !dropped => {
                    dropped = true;
                    Tamper::Drop
                }
                frame => Tamper::Pass(frame),
            }));
        }
        w.pump_until(|w| w.phone.pending_n.is_some());
        let n = w.phone.pending_n;
        w.auto_final_check = false;
        match cut {
            "unplug" => w.cut_device(0),
            "app restart" => w.cut_app(true),
            _ => {
                // Plugged in, nothing moves (the device waits on the phone, the phone on the
                // device round) until the user unplugs and plugs in again.
                w.pump();
                assert_eq!(w.state(), Some(CardState::Proving), "{cut}");
                w.cut_device(0);
            }
        }
        w.events.clear();
        w.pump();
        assert!(
            !w.events
                .iter()
                .any(|e| matches!(e, CardEvent::ReadyForFinalCheck)),
            "{cut}: no step back to [Continue]"
        );
        assert_eq!(w.phone.pending_n, n, "{cut}: the same N");
        w.assert_passed(&format!("a lost FinalBegin, {cut}"));
    }

    // The phone half written and its quiz started: the quiz again from item 1.
    let mut w = World::new(2, 2, 22);
    w.phone_holds = Some(|e| matches!(e, CardEvent::SelfCheckQuestion { k: 3, .. }));
    w.pump();
    w.phone_holds = None;
    w.cut_device(0);
    w.events.clear();
    w.pump_until(|w| {
        w.events
            .iter()
            .any(|e| matches!(e, CardEvent::SelfCheckQuestion { .. }))
    });
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::SelfCheckQuestion { k: 0, .. })));
    w.pump();
    w.assert_passed("from the quiz");
}

/// A replug while a page of words 33–64 stayed live (design §3.7): the phone's quiz keeps its
/// place, and so does the confirm page. Only a new flow starts the quiz again.
#[test]
fn a_live_replug_keeps_the_phone_place() {
    let mut w = World::new(2, 2, 28);
    w.auto_continue = false;
    w.phone_holds = Some(|e| matches!(e, CardEvent::SelfCheckQuestion { k: 3, .. }));
    w.pump();
    w.phone_holds = None;
    w.cut_device(0);
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Replug);
    w.pump();
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::CardFinished)));
    assert!(
        !w.events.iter().any(|e| matches!(
            e,
            CardEvent::SelfCheckQuestion { .. } | CardEvent::PhonePage { .. }
        )),
        "{:?}",
        w.events
    );
    // The question on screen is still item 4, word 36.
    w.phone_actions
        .push_back(PhoneAction::SelfCheck(w.truth[HALF + 3]));
    w.pump();
    w.assert_passed("from the live quiz");

    let mut w = World::new(2, 2, 29);
    w.auto_continue = false;
    w.phone_holds = Some(|e| matches!(e, CardEvent::PhonePage { page: 11, .. }));
    w.pump();
    w.phone_holds = None;
    w.cut_device(0);
    w.events.clear();
    w.phone_actions.push_back(PhoneAction::Replug);
    w.pump();
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::CardFinished)));
    assert!(
        !w.events
            .iter()
            .any(|e| matches!(e, CardEvent::PhonePage { .. })),
        "{:?}",
        w.events
    );
    w.phone_actions.push_back(PhoneAction::HalfWritten);
    w.pump();
    w.assert_passed("from the live confirm page");
}

/// After N, a bad opening of D twice is an abort: FAILED, the device suspect, and the device
/// told (spec L1.7.6).
#[test]
fn a_bad_opening_of_d_aborts() {
    let mut w = World::new(2, 2, 20);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::OpeningPart(CardOpeningPart::Carries { j, y, carries }))
            if j < 32 =>
        {
            Tamper::Pass(Frame::ToPhone(DeviceCard::OpeningPart(
                CardOpeningPart::Carries {
                    j,
                    y: (y + 7) % 2048,
                    carries,
                },
            )))
        }
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    w.assert_aborted(CardFailReason::BadOpening);
}

/// Build plan C16: a bit proof that fails twice comes after words 1–32 are written, so it's a
/// device fault (FAILED, BadProof, the device suspect), not another card; N never left the
/// phone.
#[test]
fn bad_proof_after_writing_is_a_device_fault() {
    let mut w = World::new(2, 2, 44);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::BitProofs {
            from: 0,
            mut proofs,
        }) => {
            proofs[0][95] ^= 1;
            Tamper::Pass(Frame::ToPhone(DeviceCard::BitProofs { from: 0, proofs }))
        }
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(
        w.events.iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                reason: CardFailReason::BadProof,
                device_suspect: true,
                after_n: false
            }
        )),
        "{:?}",
        w.events
    );
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::DifferentCard)));
    // Owner B8: N went out with the confirm, but the device round never started.
    assert!(w.phone.n.is_some());
    assert_eq!(w.phone.pending_n, None, "no device round");
}

/// core-protocol F2: the second strike is keyed on the device, not on the frame's own `from`.
/// A device that answers each pull with a bad frame, naming a different range each time and
/// with a frame of proofs already verified between them, still fails the card on its second
/// bad frame, as a device fault with the device suspect. Every other frame is honest, so the
/// card fails there or never: a good frame (here the replay) doesn't clear the strike
/// (review T1).
#[test]
fn a_bad_proof_strike_cant_be_dodged_by_from() {
    let mut w = World::new(2, 2, 45);
    let tampered = std::rc::Rc::new(std::cell::Cell::new(0u16));
    let counted = tampered.clone();
    let mut frames = 0u16;
    let mut good: Option<DeviceCard> = None;
    w.filter = Some(Box::new(move |frame| match frame {
        Frame::ToPhone(DeviceCard::BitProofs { from, mut proofs }) => {
            frames += 1;
            match frames {
                // The first frame passes, and is kept to send again later.
                1 => {
                    good = Some(DeviceCard::BitProofs {
                        from,
                        proofs: proofs.clone(),
                    });
                    Tamper::Pass(Frame::ToPhone(DeviceCard::BitProofs { from, proofs }))
                }
                // A frame already verified, between the two bad ones.
                3 => Tamper::Pass(Frame::ToPhone(good.take().expect("kept"))),
                2 | 4 => {
                    counted.set(counted.get() + 1);
                    proofs[0][95] ^= 1;
                    Tamper::Pass(Frame::ToPhone(DeviceCard::BitProofs {
                        from: from + counted.get(),
                        proofs,
                    }))
                }
                _ => Tamper::Pass(Frame::ToPhone(DeviceCard::BitProofs { from, proofs })),
            }
        }
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(tampered.get(), 2, "it failed on the second bad frame");
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(
        w.events.iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                reason: CardFailReason::BadProof,
                device_suspect: true,
                ..
            }
        )),
        "{:?}",
        w.events
    );
}

/// Owner B8: N is drawn and persisted with the confirm of words 1–32 (after `Finished` and
/// `DeviceHalfWritten`), and goes out as `OpenStart` right after the confirm. The device
/// computes the 5 openings after the bit proofs, and the phone verifies them all before the
/// device round, so the phone round starts without waiting. The device round runs on the same
/// N, at [Continue].
#[test]
fn n_is_sent_after_the_confirm() {
    let mut w = World::new(2, 2, 43);
    // The user writes words 33–64 for longer than the openings take: [Continue] comes once
    // everything else is quiet.
    w.auto_final_check = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::Proving));
    assert_eq!(
        w.record().unwrap().dots(),
        5,
        "all 5 openings left in PROVING"
    );
    w.auto_final_check = true;
    w.phone_actions.push_back(PhoneAction::FinalCheck);
    w.pump();
    w.assert_passed("N after the confirm");
    let log = &w.phone.log;
    let at = |matches: fn(&CoordCardMutation) -> bool| log.iter().position(matches).unwrap();
    let confirm = at(|m| matches!(m, CoordCardMutation::DeviceHalfWritten { .. }));
    let started = at(|m| matches!(m, CoordCardMutation::CheckStarted { .. }));
    let checked = at(|m| matches!(m, CoordCardMutation::Checked { .. }));
    let last_proof = log
        .iter()
        .rposition(|m| matches!(m, CoordCardMutation::ProofsVerified { .. }))
        .unwrap();
    let pending = at(|m| matches!(m, CoordCardMutation::DeviceRoundPending { .. }));
    let result = at(|m| matches!(m, CoordCardMutation::DeviceRoundResult { pass: true, .. }));
    assert!(confirm < started && started < pending, "{log:?}");
    assert!(
        checked < pending && last_proof < pending && pending < result,
        "{log:?}"
    );
    let n = w.phone.n.unwrap();
    let frame = |matches: &dyn Fn(&Frame) -> bool| w.frames.iter().position(matches).unwrap();
    let first_n = frame(&|f| matches!(f, Frame::ToDevice(m) if contains(&encoded(m), &n)));
    assert!(matches!(
        w.frames[first_n],
        Frame::ToDevice(CoordinatorCard::OpenStart { .. })
    ));
    let confirm_frame = frame(&|f| {
        matches!(
            f,
            Frame::ToDevice(CoordinatorCard::DeviceHalfWritten { .. })
        )
    });
    let final_begin = frame(&|f| matches!(f, Frame::ToDevice(CoordinatorCard::FinalBegin)));
    let round = frame(&|f| matches!(f, Frame::ToPhone(DeviceCard::DeviceRound { pass: true })));
    assert!(confirm_frame < first_n && first_n < final_begin && final_begin < round);
    // All 15 parts of the 5 openings reached the phone before the device round passed.
    let parts_before_round = w.frames[..round]
        .iter()
        .filter(|f| matches!(f, Frame::ToPhone(DeviceCard::OpeningPart(_))))
        .count();
    assert_eq!(
        parts_before_round,
        3 * CHECK_BOXES,
        "openings done before the phone round"
    );
    // ... after the bit proofs.
    let last_proofs = w
        .frames
        .iter()
        .rposition(|f| matches!(f, Frame::ToPhone(DeviceCard::BitProofs { .. })))
        .unwrap();
    let first_part = frame(&|f| matches!(f, Frame::ToPhone(DeviceCard::OpeningPart(_))));
    assert!(last_proofs < first_part);
    // The phone round asked its first question with no opening requested after the round.
    assert!(!w.frames[round..]
        .iter()
        .any(|f| matches!(f, Frame::ToDevice(CoordinatorCard::OpenBox { .. }))));
}

/// Binding (2), spec PK.6 and build plan A3: every re-encode is checked against the persisted
/// words hash before any word or P leaves the chip. With the record's hash changed, the next
/// encode refuses with `DeviceError`, stops the card job and shows no word, and words 33–64
/// aren't sent.
#[test]
fn words_hash_mismatch_stops_the_card() {
    let mut w = World::new(2, 2, 45);
    let flash = w.run.start_devices[&w.id].clone();
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceHalf { .. })));
    let log = w
        .device_log
        .iter()
        .cloned()
        .map(|m| match m {
            DeviceCardMutation::SealedC {
                share_image,
                sealed,
                mut words_hash,
            } => {
                words_hash[0] ^= 1;
                DeviceCardMutation::SealedC {
                    share_image,
                    sealed,
                    words_hash,
                }
            }
            m => m,
        })
        .collect::<Vec<_>>();
    w.replace_device(flash, log);
    let (frames, screens) = (w.frames.len(), w.screens.len());
    w.pump();
    assert!(w.frames[frames..].iter().any(|f| matches!(
        f,
        Frame::ToPhone(DeviceCard::Refused {
            reason: CardRefusal::DeviceError
        })
    )));
    assert!(w.screens[screens..]
        .iter()
        .any(|s| matches!(s, CardScreen::DeviceError)));
    assert!(!w.screens[screens..].iter().any(|s| matches!(
        s,
        CardScreen::DeviceHalf { .. } | CardScreen::SelfCheck { .. }
    )));
    assert!(!w.frames[frames..]
        .iter()
        .any(|f| matches!(f, Frame::ToPhone(DeviceCard::PhoneHalf(_)))));
    assert!(w.session.is_none(), "the session closed");
}

/// Two misses on the same word of D abort; one is a second look.
#[test]
fn phone_round_misses() {
    let mut w = World::new(2, 2, 21);
    w.misses = 1;
    w.pump();
    w.assert_passed("one miss");
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::TryAgain { .. })));
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Question {
            second_look: true,
            ..
        }
    )));

    let mut w = World::new(2, 2, 22);
    w.misses = 2;
    w.pump();
    w.assert_aborted(CardFailReason::TwoMisses);
}

/// A device round with a miss and a second look passes; two misses fail it before N.
#[test]
fn device_round_misses() {
    let mut w = World::new(2, 2, 23);
    w.round_misses.insert(2, 1);
    w.pump();
    w.assert_passed("one miss");
    assert!(w
        .screens
        .iter()
        .any(|s| matches!(s, CardScreen::DeviceRoundMismatch { .. })));
    let same_lookalikes = |w: &World| {
        let shown: Vec<[u16; 3]> = w
            .screens
            .iter()
            .filter_map(|s| match s {
                CardScreen::DeviceRoundAnswer { k: 2, options, .. } => {
                    let mut options = options.0;
                    options.sort();
                    Some(options)
                }
                _ => None,
            })
            .collect();
        assert_eq!(shown.len(), 2, "a first and a second look");
        assert_eq!(shown[0], shown[1], "the second look keeps its lookalikes");
    };
    same_lookalikes(&w);

    let mut w = World::new(2, 2, 24);
    w.round_misses.insert(2, 2);
    w.pump();
    same_lookalikes(&w);
    assert_eq!(
        w.state(),
        Some(CardState::Failed(VerdictReason::DeviceRoundFailed))
    );
    assert_eq!(w.status(), CardBackupStatus::Failed);
    // Owner B8: after N a failed device round leaves the device out too.
    assert!(w.suspect());
}

/// Owner B12 and its follow-up 5: the phone has no Stop. Once N is out, while words 33–64 are
/// written and in the device round, a `Stop` that reaches the device anyway is refused and
/// changes nothing: the card goes on and passes.
#[test]
fn a_stop_after_n_is_refused_by_the_device() {
    type Point = fn(&mut World) -> bool;
    let points: [(&str, Point); 2] = [
        ("words 33–64", |w| {
            w.record().is_some_and(|rec| rec.n_tag.is_some())
                && w.events
                    .iter()
                    .any(|e| matches!(e, CardEvent::PhonePage { page: 2, .. }))
        }),
        ("the device round", |w| {
            matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { .. }))
        }),
    ];
    for (point, reached) in points {
        let mut w = World::new(2, 2, 47);
        w.pump_until(reached);
        assert!(
            w.record().unwrap().n_tag.is_some(),
            "{point}: the device took N"
        );
        let state = w.state();
        let pending = std::mem::take(&mut w.wire);
        let replies = w.device_replies(CoordinatorCard::Stop);
        assert!(
            matches!(
                replies.as_slice(),
                [DeviceCard::Refused {
                    reason: CardRefusal::WrongState
                }]
            ),
            "{point}: {replies:?}"
        );
        assert_eq!(w.state(), state, "{point}: the device carries on");
        w.wire = pending;
        w.pump();
        w.assert_passed(point);
    }
}

/// Owner B12: a device that reports a Stop after N anyway is treated as malicious. Its FAILED
/// verdict in the device round (where an honest device has no Stop), or a status that says it
/// stopped once this phone sent N, is an abort with the device suspect.
#[test]
fn a_device_reporting_a_stop_after_n_is_suspect() {
    let mut w = World::new(2, 2, 49);
    w.auto_final_check = false;
    w.pump();
    w.cut_device(0);
    w.auto_final_check = true;
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { .. })));
    chip_takes_over(&mut w);
    chip_says(&mut w, DeviceCard::Verdict { passed: false }).unwrap();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Failed {
            device_suspect: true,
            ..
        }
    )));

    let mut w = World::new(2, 2, 50);
    w.pump_until(|w| {
        w.record().is_some_and(|rec| rec.n_tag.is_some())
            && w.events
                .iter()
                .any(|e| matches!(e, CardEvent::PhonePage { page: 1, .. }))
    });
    let mut status = chip_takes_over(&mut w);
    status.state = CardStateReport::Failed;
    status.check = CardCheckReport::Ended { passed: false };
    status.reason = Some(VerdictReason::StoppedBeforeN);
    // The chip drops its session (an interruption, owner B9); the user says it was them.
    let sent = chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        },
    )
    .unwrap();
    assert!(sent.iter().any(|m| matches!(m, CoordinatorCard::Begin(_))));
    chip_says(&mut w, DeviceCard::Status(Box::new(status))).unwrap();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Failed {
            device_suspect: true,
            ..
        }
    )));
}

/// T6: with a dead RNG on the device the card still passes: every nonce is hedged.
#[test]
fn dead_rng() {
    let mut w = World::new(2, 2, 26);
    w.device_rng = DeviceRng(None);
    w.fw_rng = DeviceRng(None);
    w.pump();
    w.assert_passed("dead rng");
}

/// Design §3.11 #16: the phone's events and its RAM words show no word in their `Debug`.
#[test]
fn events_are_redacted() {
    let mut w = World::new(2, 2, 27);
    w.pump();
    for event in &w.events {
        let printed = format!("{event:?}");
        for word in w.truth.iter().map(|&y| BIP39_WORDS[y as usize]) {
            assert!(!printed.contains(word), "{printed}");
        }
    }
    for frame in &w.frames {
        let printed = format!("{frame:?}");
        for word in w.truth.iter().map(|&y| BIP39_WORDS[y as usize]) {
            assert!(!printed.contains(word), "{printed}");
        }
    }
}

/// The phone's side of a card restore, and the user who works both screens.
struct Restorer<'a> {
    run: &'a mut Run,
    id: DeviceId,
    rng: StdRng,
    /// The device's states and outcomes as the restore page hears them.
    states: Vec<CardRestoreEvent>,
    screens: Vec<CardScreen>,
    decodes: Vec<(
        CardRestorePhase,
        u32,
        frostsnap_core::device::card_restore::CardEntries,
    )>,
    reports: Vec<CardRestoreReport>,
    entry: Option<CardRestorePhase>,
}

impl Restorer<'_> {
    fn new(run: &mut Run, id: DeviceId, seed: u64) -> Restorer<'_> {
        Restorer {
            run,
            id,
            rng: StdRng::seed_from_u64(seed),
            states: vec![],
            screens: vec![],
            decodes: vec![],
            reports: vec![],
            entry: None,
        }
    }

    fn handle(&mut self, sends: Vec<DeviceSend>) {
        for send in sends {
            match send {
                DeviceSend::ToCoordinator(message) => {
                    let DeviceToCoordinatorMessage::Card(_) = &*message else {
                        self.run
                            .extend_from_device(self.id, [DeviceSend::ToCoordinator(message)]);
                        continue;
                    };
                    let sends = self
                        .run
                        .coordinator
                        .recv_device_message(self.id, *message)
                        .unwrap();
                    for send in sends {
                        match send {
                            CoordinatorSend::ToUser(CoordinatorToUserMessage::CardRestore(
                                restore,
                            )) => {
                                assert_eq!(restore.device_id, self.id);
                                self.states.push(restore.event)
                            }
                            other => panic!("only the restore page hears: {other:?}"),
                        }
                    }
                }
                DeviceSend::ToUser(message) => match *message {
                    DeviceToUserMessage::Card(card) => match *card {
                        ToUserCard::RestoreEntry(phase) => self.entry = Some(phase),
                        ToUserCard::Screen(screen) => self.screens.push(screen),
                        ToUserCard::RestoreDecode {
                            phase,
                            generation,
                            entries,
                        } => self.decodes.push((phase, generation, *entries)),
                        ToUserCard::RestoreReport(report) => self.reports.push(*report),
                        other => panic!("unexpected {other:?}"),
                    },
                    other => self
                        .run
                        .extend_from_device(self.id, [DeviceSend::ToUser(Box::new(other))]),
                },
            }
        }
    }

    /// `EnterCard` to the device, as the restore page sends it.
    fn enter_card(&mut self, target: Option<SharedKey>) -> EnterPhysicalId {
        self.enter_card_with(target, None)
    }

    /// `EnterCard` with the wallet's share indices (build plan A6).
    fn enter_card_with(
        &mut self,
        target: Option<SharedKey>,
        share_indices: Option<Vec<u8>>,
    ) -> EnterPhysicalId {
        let id = EnterPhysicalId::new(&mut self.rng);
        let enter = self.run.coordinator.tell_device_to_load_card(
            id,
            self.id,
            target,
            Some(proof::FORMAT_ID),
            share_indices,
        );
        let CoordinatorSend::ToDevice { message, .. } = enter else {
            panic!("EnterCard")
        };
        let sends = self
            .run
            .device(self.id)
            .recv_coordinator_message(message, &mut StdRng::seed_from_u64(0))
            .unwrap();
        self.handle(sends);
        id
    }

    /// The app's words 33–64.
    fn phone_half(&mut self, id: EnterPhysicalId, words: [Option<u16>; HALF]) {
        let message =
            CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::CardPhoneHalf {
                enter_physical_id: id,
                words: CardHalfEntries(words),
            });
        let sends = self
            .run
            .device(self.id)
            .recv_coordinator_message(message, &mut StdRng::seed_from_u64(0))
            .unwrap();
        self.handle(sends);
    }

    /// The user confirms words 1–32 on the device.
    fn device_half(&mut self, words: [Option<u16>; HALF]) {
        let phase = self.entry.clone().expect("the entry is open");
        let sends = self
            .run
            .device(self.id)
            .card_restore_device_half(&phase, words);
        self.handle(sends);
    }

    fn last_state(&self) -> (bool, bool, bool) {
        match self.states.last() {
            Some(CardRestoreEvent::State {
                device_half_in,
                phone_half_in,
                decoding,
            }) => (*device_half_in, *phone_half_in, *decoding),
            other => panic!("expected a state, got {other:?}"),
        }
    }

    /// The firmware's decode of the latest `RestoreDecode`, run to the end, at T_dec.
    fn decode(&mut self) -> Option<CardRestoreReport> {
        let (phase, generation, entries) = self.decodes.pop().expect("a decode");
        let mut job = RestoreJob::new(
            &entries,
            phase.target.as_ref(),
            phase.recorded_format,
            phase.share_indices.as_deref(),
        );
        let (outcome, _) = run_restore_job(&mut job);
        let sends = self
            .run
            .device(self.id)
            .card_restore_decoded(generation, outcome);
        self.handle(sends);
        self.reports.pop()
    }
}

/// The simulated length of one decode slice: about what one takes on the ESP32-C3 (64
/// prefilter checks, a rebuild and a read of the self masks over 256 counters).
const SIM_SLICE_MS: u64 = 30;

/// A decode's slices as the firmware runs them, on a simulated clock that moves
/// [`SIM_SLICE_MS`] per slice: the outcome, and the work of each slice up to and including the
/// one the outcome came out of.
fn run_restore_job(job: &mut RestoreJob) -> (RestoreOutcome, Vec<Work>) {
    let mut elapsed = 0;
    let mut slices = vec![];
    loop {
        let before = job.work();
        let outcome = job.step(elapsed);
        slices.push(job.work().since(&before));
        if let Some(outcome) = outcome {
            assert!(
                elapsed >= RESTORE_DECODE_MS,
                "nothing comes out before T_dec"
            );
            assert!(
                job.step(elapsed + SIM_SLICE_MS).is_none(),
                "it comes out once"
            );
            return (outcome, slices);
        }
        elapsed += SIM_SLICE_MS;
        assert!(elapsed < 2 * RESTORE_DECODE_MS);
    }
}

/// The firmware's part after the report: the consolidation of a restored share, after its
/// threshold warning's hold when there is one (build plan A11).
#[derive(Default)]
struct RestoreEnv {
    entered: Vec<PhysicalBackupPhase>,
    /// Each consolidation's threshold warning, in order.
    threshold_warnings: Vec<Option<ThresholdWarning>>,
}

impl Env for RestoreEnv {
    fn user_react_to_coordinator(
        &mut self,
        _run: &mut Run,
        message: CoordinatorToUserMessage,
        _rng: &mut impl RngCore,
    ) {
        if let CoordinatorToUserMessage::Restoration(
            CoordToUserRestoration::PhysicalBackupEntered(phase),
        ) = message
        {
            self.entered.push(*phase);
        }
    }

    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        rng: &mut impl RngCore,
    ) {
        if let DeviceToUserMessage::Restoration(restoration) = message {
            if let ToUserRestoration::ConsolidateBackup(phase) = *restoration {
                self.threshold_warnings.push(phase.threshold_warning);
                let ack = run
                    .device(from)
                    .finish_consolidation(&mut TestDeviceKeyGen, phase, rng);
                run.extend_from_device(from, ack);
            }
        }
    }
}

/// A card key's shares: (device, image, words).
type Cards = Vec<(DeviceId, ShareImage, [u16; WORDS])>;

/// The cards of a 2-of-2 card key's shares.
fn cards(run: &mut Run) -> (AccessStructureRef, SharedKey, Cards) {
    let asref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let root = run
        .coordinator
        .root_shared_key(asref, TEST_ENCRYPTION_KEY)
        .unwrap();
    let shares = run
        .coordinator
        .get_access_structure(asref)
        .unwrap()
        .device_to_share_indicies();
    let cards = shares
        .iter()
        .map(|(&id, &index)| {
            let contrib = CoordShareDecryptionContrib::for_master_share(id, index, &root);
            let share = run
                .device(id)
                .get_encrypted_share(asref, index)
                .unwrap()
                .decrypt(asref, contrib, &mut TestDeviceKeyGen)
                .unwrap();
            (
                id,
                share.share_image(),
                codec::encode(&share, &root).unwrap().words,
            )
        })
        .collect();
    (asref, root, cards)
}

fn halves(words: &[u16; WORDS]) -> ([Option<u16>; HALF], [Option<u16>; HALF]) {
    (
        core::array::from_fn(|j| Some(words[j])),
        core::array::from_fn(|j| Some(words[HALF + j])),
    )
}

/// Design §3.11 #9, #14: a fresh recovery from two cards, each entered as two halves (in both
/// orders, one with a wrong word, one with an unreadable one), then unique decoding, `Save3`, and
/// the restoration finishes and consolidates as for 25-word backups. Every `RestoreState`
/// follows a change; a phone half before `EnterCard` is dropped.
#[test]
fn restore_from_halves() {
    let mut rng = StdRng::seed_from_u64(30);
    let mut run = key(2, 2, 30);
    let (asref, root, cards) = cards(&mut run);
    run.clear_coordinator();
    let mut env = RestoreEnv::default();
    let restoration_id = RestorationId::new(&mut rng);
    run.coordinator
        .start_restoring_key("card key".into(), None, KeyPurpose::Test, restoration_id);
    let mut blanks = vec![];
    for (i, &(_, image, words)) in cards.iter().enumerate() {
        let blank = run.new_device(&mut rng);
        blanks.push((blank, image));
        let (device, mut phone) = halves(&words);
        let mut device = device;
        if i == 0 {
            device[9] = Some((words[9] + 3) % 2048);
        } else {
            phone[20] = None;
        }
        let mut r = Restorer::new(&mut run, blank, 31 + i as u64);
        // A half before any `EnterCard` is dropped.
        r.phone_half(EnterPhysicalId::new(&mut StdRng::seed_from_u64(9)), phone);
        assert!(r.states.is_empty());
        let id = r.enter_card(None);
        assert_eq!(r.last_state(), (false, false, false), "the ack");
        if i == 0 {
            r.phone_half(id, phone);
            assert_eq!(r.last_state(), (false, true, false));
            r.device_half(device);
        } else {
            r.device_half(device);
            assert_eq!(r.last_state(), (true, false, false));
            assert_eq!(r.screens.last(), Some(&CardScreen::WaitingForPhoneHalf));
            r.phone_half(id, phone);
        }
        assert_eq!(r.last_state(), (true, true, true), "both in: decoding");
        assert_eq!(r.screens.last(), Some(&CardScreen::RestoreChecking));
        let report = r.decode().expect("a report");
        assert!(matches!(
            r.states.iter().rev().nth(1),
            Some(CardRestoreEvent::Outcome {
                result: RestoreResult::Restored,
            })
        ));
        assert_eq!(r.last_state(), (true, true, false));
        let CardRestoreReport::Restored { decoded, findings } = report else {
            panic!("expected a restore, got {report:?}")
        };
        if i == 0 {
            assert_eq!(findings.corrected, 1 << 9);
        } else {
            assert_eq!(findings.erased, 1 << (HALF + 20));
        }
        let sends = r.run.device(blank).card_restore_confirmed(decoded);
        r.handle(sends);
        run.run_until_finished(&mut env, &mut rng).unwrap();
        let phase = env.entered.pop().expect("the card was entered");
        assert_eq!(phase.backup.share_image, image);
        run.coordinator
            .check_physical_backup_compatible_with_restoration(
                restoration_id,
                phase,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        let save = run
            .coordinator
            .tell_device_to_save_physical_backup(phase, restoration_id);
        run.extend(save);
        run.run_until_finished(&mut env, &mut rng).unwrap();
        let saved = &run.device(blank).saved_backups()[&image];
        assert!(matches!(
            saved.backup,
            PhysicalBackup::Card {
                threshold: Some(2),
                ..
            }
        ));
    }
    let restored = run
        .coordinator
        .finish_restoring(restoration_id, TEST_ENCRYPTION_KEY, &mut rng)
        .unwrap();
    assert_eq!(restored, asref);
    for &(blank, _) in &blanks {
        for pending in run.coordinator.pending_physical_consolidations(blank) {
            let consolidate = run
                .coordinator
                .consolidate_pending_physical_backups(blank, pending, TEST_ENCRYPTION_KEY)
                .unwrap();
            run.extend(consolidate);
        }
    }
    run.run_until_finished(&mut env, &mut rng).unwrap();
    for &(blank, image) in &blanks {
        let device = run.device(blank);
        assert!(device.get_encrypted_share(asref, image.index).is_some());
        assert_eq!(
            device.card_record(&image).unwrap().origin,
            ShareOrigin::Restored { card: true }
        );
        // One card per share (design §0): a restored share gets no second card.
        let begin = CoordinatorCard::Begin(Box::new(CardBegin {
            share_index: image.index,
            root_shared_key: root.clone(),
            format_id: proof::FORMAT_ID,
            proof_version: proof::PROOF_VERSION,
        }));
        let sends = device.recv_card_message(begin, None);
        assert!(sends.iter().any(|send| matches!(
            send,
            DeviceSend::ToCoordinator(m) if matches!(
                **m,
                DeviceToCoordinatorMessage::Card(DeviceCard::Refused {
                    reason: CardRefusal::NotKeygenOrigin
                })
            )
        )));
    }
    // The same card again on a device that now holds it: refused, and the page is told why
    // rather than "couldn't be read".
    let (blank, _) = blanks[0];
    let (device, phone) = halves(&cards[0].2);
    let mut r = Restorer::new(&mut run, blank, 40);
    let id = r.enter_card(None);
    r.phone_half(id, phone);
    r.device_half(device);
    assert!(matches!(r.decode(), Some(CardRestoreReport::AlreadyHeld)));
    assert!(matches!(
        r.states.iter().rev().nth(1),
        Some(CardRestoreEvent::Outcome {
            result: RestoreResult::AlreadyHeld,
        })
    ));
    run.check_mutations();
}

/// Design §3.11 #9, #14: two wrong words are repaired against the wallet's F, and not without
/// it; an edit of either half during a decode discards that result and decodes the new halves,
/// with exactly one `RestoreOutcome`; a reconnect between the halves ends with both in and one
/// decode.
#[test]
fn restore_repairs_and_edits() {
    let mut run = key(2, 2, 32);
    let (_asref, root, cards) = cards(&mut run);
    let (_, _, words) = cards[0];
    let blank = run.new_device(&mut StdRng::seed_from_u64(33));
    let (device, mut phone) = halves(&words);
    // Two wrong words, one in each half.
    let mut device_wrong = device;
    device_wrong[3] = Some((words[3] + 1) % 2048);
    phone[30] = Some((words[HALF + 30] + 1) % 2048);

    // Without F: unique decoding can't, so it fails, and the report says so.
    let mut r = Restorer::new(&mut run, blank, 34);
    let id = r.enter_card(None);
    r.phone_half(id, phone);
    r.device_half(device_wrong);
    assert!(matches!(r.decode(), Some(CardRestoreReport::Failed(_))));
    assert!(matches!(
        r.states.iter().rev().nth(1),
        Some(CardRestoreEvent::Outcome {
            result: RestoreResult::CouldNotRead,
            ..
        })
    ));

    // With F: repaired.
    let id = r.enter_card(Some(root.clone()));
    r.phone_half(id, phone);
    r.device_half(device_wrong);
    let Some(CardRestoreReport::Restored { findings, .. }) = r.decode() else {
        panic!("repaired")
    };
    assert_eq!(findings.corrected, 1 << 3 | 1 << (HALF + 30));

    // Three unreadable words use every spare word: the masks still confirm the share, with F
    // or without (owner B7), so there's nothing left to warn about.
    let (mut erased_device, mut erased_phone) = halves(&words);
    erased_device[1] = None;
    erased_device[2] = None;
    erased_phone[5] = None;
    for target in [None, Some(root.clone())] {
        let id = r.enter_card(target);
        r.phone_half(id, erased_phone);
        r.device_half(erased_device);
        let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
            panic!("restored")
        };
        assert_eq!(decoded.backup.share_image(), cards[0].1);
        assert_eq!(findings.erased, 1 << 1 | 1 << 2 | 1 << (HALF + 5));
    }
    // Past the spare words with no F, a wrong word decodes uniquely to another codeword or to
    // none; either way its masks fail and the card isn't read, rather than a wrong share.
    let mut miscorrected = erased_device;
    miscorrected[2] = None;
    miscorrected[3] = Some((words[3] + 1) % 2048);
    let id = r.enter_card(None);
    r.phone_half(id, erased_phone);
    r.device_half(miscorrected);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
    ));

    // An edit of the phone half while the decode runs: its result is discarded, and the new
    // halves decode, with one outcome in all.
    let id = r.enter_card(None);
    r.phone_half(id, phone);
    r.device_half(device);
    let outcomes = |r: &Restorer| {
        r.states
            .iter()
            .filter(|s| matches!(s, CardRestoreEvent::Outcome { .. }))
            .count()
    };
    let before = outcomes(&r);
    let (_, fixed) = halves(&words);
    r.phone_half(id, fixed);
    assert_eq!(r.decodes.len(), 1, "no second decode while one runs");
    assert!(r.decode().is_none(), "discarded");
    assert_eq!(r.decodes.len(), 1, "the new halves decode");
    assert_eq!(r.last_state(), (true, true, true));
    let Some(CardRestoreReport::Restored { findings, .. }) = r.decode() else {
        panic!("restored")
    };
    assert_eq!(findings.corrected, 0);
    assert_eq!(outcomes(&r), before + 1);

    // A reconnect between the halves: the device reboots with the phone half in; the app's
    // `EnterCard` again, acknowledged, and the half again.
    let id = r.enter_card(None);
    r.phone_half(id, fixed);
    r.run.device(blank).clear_tmp_data();
    let id = r.enter_card(None);
    assert_eq!(r.last_state(), (false, false, false));
    r.phone_half(id, fixed);
    r.device_half(device);
    assert_eq!(r.decodes.len(), 1);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Restored { .. })
    ));
}

/// The 61 values of each share's card, in the order [`cards`] gives the shares.
fn card_values(run: &mut Run, asref: AccessStructureRef, root: &SharedKey) -> Vec<[u16; COEFFS]> {
    let shares = run
        .coordinator
        .get_access_structure(asref)
        .unwrap()
        .device_to_share_indicies();
    shares
        .iter()
        .map(|(&id, &index)| {
            let contrib = CoordShareDecryptionContrib::for_master_share(id, index, root);
            let share = run
                .device(id)
                .get_encrypted_share(asref, index)
                .unwrap()
                .decrypt(asref, contrib, &mut TestDeviceKeyGen)
                .unwrap();
            codec::encode(&share, root).unwrap().values
        })
        .collect()
}

/// The 64 words a chip writes from these values: the device half computed from their committed
/// bits and index, and `phone` as words 33–64. This is what a lying chip writes when it garbles
/// a value the proofs don't bind (the threshold value, a mask) or picks its own words 33–64: its
/// seal, proofs and openings are all over these bits, so they hold (spec PK.2).
fn words_from_values(values: &[u16; COEFFS], phone: &[u16; HALF]) -> [u16; WORDS] {
    layout::words_from_bits(
        &layout::committed_bits(values),
        layout::index_of(values),
        phone,
    )
}

fn phone_of(values: &[u16; COEFFS]) -> [u16; HALF] {
    values[FIRST_PHONE_SLOT..FIRST_SHARE_SLOT]
        .try_into()
        .unwrap()
}

/// Spec PK.3, PK.8 (the threshold is warn-only): a card whose threshold value reads 3 for a
/// 2-of-2 wallet (a chip garbled it, its masks are the KDF's) restores against F, by unique
/// decoding and by repair, with the threshold warning; `RestoreOutcome` carries only the result
/// (owner B7). Without F it restores with the card's threshold and nothing to compare it with.
#[test]
fn restore_warns_on_threshold_mismatch() {
    let mut run = key(2, 2, 50);
    let (asref, root, cards) = cards(&mut run);
    let values = card_values(&mut run, asref, &root);
    let (_, image, words) = cards[0];
    assert_eq!(words_from_values(&values[0], &phone_of(&values[0])), words);
    let mut garbled = values[0];
    garbled[THRESHOLD_SLOT] ^= 1;
    let lying = words_from_values(&garbled, &phone_of(&garbled));
    let blank = run.new_device(&mut StdRng::seed_from_u64(51));
    let mut r = Restorer::new(&mut run, blank, 52);
    let (device, phone) = halves(&lying);
    let mut device_wrong = device;
    device_wrong[3] = Some((lying[3] + 1) % 2048);
    let mut phone_wrong = phone;
    phone_wrong[30] = Some((lying[HALF + 30] + 1) % 2048);
    for (device, phone, corrected) in [
        (device, phone, 0),
        (device_wrong, phone_wrong, 1 << 3 | 1 << (HALF + 30)),
    ] {
        let id = r.enter_card(Some(root.clone()));
        r.phone_half(id, phone);
        r.device_half(device);
        let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
            panic!("restored with a warning")
        };
        assert_eq!(decoded.backup.share_image(), image);
        assert_eq!(findings.corrected, corrected);
        assert_eq!(findings.threshold, Some(3));
        assert!(findings.threshold_differs);
        assert_eq!(findings.canonical, Some(false));
        assert!(matches!(
            r.states.iter().rev().nth(1),
            Some(CardRestoreEvent::Outcome {
                result: RestoreResult::Restored,
            })
        ));
    }
    let id = r.enter_card(None);
    r.phone_half(id, phone);
    r.device_half(device);
    let Some(CardRestoreReport::Restored { findings, .. }) = r.decode() else {
        panic!("restored")
    };
    assert_eq!(findings.threshold, Some(3));
    assert!(!findings.threshold_differs, "no wallet to differ from");
    assert_eq!(findings.canonical, None);
}

/// Spec PK.3, PK.11 (the tlow attack inverted) and build plan A11: a chip that garbles its
/// threshold value (unreadable on one card, 3 on the other) writes cards whose seal, proofs and
/// openings hold, since they bind only the index and the scalar. Both decode, are saved, and the
/// restored wallet **consolidates**, each card holding on its threshold warning first; it's
/// never refused.
#[test]
fn a_garbled_threshold_consolidates_with_a_warning() {
    let mut rng = StdRng::seed_from_u64(60);
    let mut run = key(2, 2, 60);
    let (asref, _root, cards) = cards(&mut run);
    let values = card_values(&mut run, asref, &_root);
    run.clear_coordinator();
    let mut env = RestoreEnv::default();
    let restoration_id = RestorationId::new(&mut rng);
    run.coordinator
        .start_restoring_key("card key".into(), None, KeyPurpose::Test, restoration_id);
    let mut blanks = vec![];
    for (i, (&(_, image, _), values)) in cards.iter().zip(&values).enumerate() {
        let mut garbled = *values;
        // Card 0: the top bit, so the threshold doesn't read. Card 1: reads 3.
        garbled[THRESHOLD_SLOT] ^= if i == 0 { 0x400 } else { 1 };
        let words = words_from_values(&garbled, &phone_of(&garbled));
        let blank = run.new_device(&mut rng);
        blanks.push((blank, image));
        let (device, phone) = halves(&words);
        let mut r = Restorer::new(&mut run, blank, 61 + i as u64);
        let id = r.enter_card(None);
        r.phone_half(id, phone);
        r.device_half(device);
        let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
            panic!("card {i} restores")
        };
        assert_eq!(findings.threshold, if i == 0 { None } else { Some(3) });
        let sends = r.run.device(blank).card_restore_confirmed(decoded);
        r.handle(sends);
        run.run_until_finished(&mut env, &mut rng).unwrap();
        let phase = env.entered.pop().expect("the card was entered");
        run.coordinator
            .check_physical_backup_compatible_with_restoration(
                restoration_id,
                phase,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        let save = run
            .coordinator
            .tell_device_to_save_physical_backup(phase, restoration_id);
        run.extend(save);
        run.run_until_finished(&mut env, &mut rng).unwrap();
        let saved = &run.device(blank).saved_backups()[&image];
        let threshold = if i == 0 { UNREADABLE_THRESHOLD } else { 3 };
        assert!(matches!(
            saved.backup,
            PhysicalBackup::Card { threshold: Some(t), .. } if t == threshold
        ));
    }
    let restored = run
        .coordinator
        .finish_restoring(restoration_id, TEST_ENCRYPTION_KEY, &mut rng)
        .unwrap();
    assert_eq!(restored, asref);
    for &(blank, _) in &blanks {
        for pending in run.coordinator.pending_physical_consolidations(blank) {
            let consolidate = run
                .coordinator
                .consolidate_pending_physical_backups(blank, pending, TEST_ENCRYPTION_KEY)
                .unwrap();
            run.extend(consolidate);
        }
        run.run_until_finished(&mut env, &mut rng).unwrap();
    }
    assert_eq!(
        env.threshold_warnings,
        [
            Some(ThresholdWarning {
                card: None,
                wallet: 2
            }),
            Some(ThresholdWarning {
                card: Some(3),
                wallet: 2
            }),
        ]
    );
    for &(blank, image) in &blanks {
        assert!(
            run.device(blank)
                .get_encrypted_share(asref, image.index)
                .is_some(),
            "consolidated"
        );
    }
}

/// Build plan A6: with the wallet's share indices in `EnterCard`, the repair prefilter keeps
/// only candidates at those indices. A card with 2 wrong words and 1 unreadable (the worst
/// repair) restores, and the search rebuilds a small fraction of the candidates it rebuilds with
/// no index set.
#[test]
fn repair_with_index_set_fits_the_budget() {
    let mut run = key(2, 2, 53);
    let (asref, root, cards) = cards(&mut run);
    let indices: Vec<u8> = run
        .coordinator
        .get_access_structure(asref)
        .unwrap()
        .device_to_share_indicies()
        .values()
        .map(|&index| u32::try_from(index).unwrap() as u8)
        .collect();
    let (_, image, words) = cards[0];
    let (mut device, mut phone) = halves(&words);
    device[30] = Some((words[30] + 1) % 2048);
    phone[31] = Some((words[HALF + 31] + 5) % 2048);
    device[20] = None;
    let blank = run.new_device(&mut StdRng::seed_from_u64(54));
    let mut r = Restorer::new(&mut run, blank, 55);
    let id = r.enter_card_with(Some(root.clone()), Some(indices.clone()));
    r.phone_half(id, phone);
    r.device_half(device);
    let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
        panic!("repaired")
    };
    assert_eq!(decoded.backup.share_image(), image);
    assert_eq!(findings.corrected, 1 << 30 | 1 << (HALF + 31));
    assert_eq!(findings.erased, 1 << 20);
    assert!(!findings.threshold_differs);

    let entries = CardEntries::from_halves(&device, &phone);
    let search = |set: Option<&[u8]>| {
        let mut table = ImageTable::new(root.point_polynomial(), set);
        while !table.build() {}
        let target = table.target().unwrap();
        let mut job = RepairJob::new(&entries.0, 2).unwrap();
        loop {
            match job.step(&target, 64) {
                Some(RepairOutcome::Found(_)) => break,
                Some(other) => panic!("{other:?}"),
                None => {}
            }
        }
        let stats = job.stats();
        (stats.subsets, stats.rebuilds)
    };
    let (subsets, with_set) = search(Some(&indices));
    let (_, without_set) = search(None);
    assert!(subsets >= 500, "{subsets} sets: too few to compare");
    assert!(with_set * 20 < without_set, "{with_set} vs {without_set}");
}

/// Build plan A6 (the set only filters repairs): a device is erased, so its share is deleted
/// from the wallet and the share-index set the app sends leaves out the card's index. Its card,
/// restored into the wallet on a blank device, still decodes against F (clean, and with a wrong
/// word and an unreadable one), and consolidates into the wallet as the app's "add to wallet" flow
/// does. Past unique decoding the set does filter: 2 wrong words aren't repaired with the
/// remaining indices, but are with a set that lists the card's index.
#[test]
fn a_deleted_share_restores_into_its_wallet() {
    let mut rng = StdRng::seed_from_u64(70);
    let mut run = key(3, 2, 70);
    let (asref, root, cards) = cards(&mut run);
    let (erased, image, words) = cards[0];
    run.coordinator.delete_share(asref, erased).unwrap();
    let indices = |run: &Run| -> Vec<u8> {
        run.coordinator
            .get_access_structure(asref)
            .unwrap()
            .device_to_share_indicies()
            .values()
            .map(|&index| u32::try_from(index).unwrap() as u8)
            .collect()
    };
    let remaining = indices(&run);
    let card_index = u32::try_from(image.index).unwrap() as u8;
    assert_eq!(remaining.len(), 2);
    assert!(!remaining.contains(&card_index));

    let blank = run.new_device(&mut StdRng::seed_from_u64(71));
    let mut r = Restorer::new(&mut run, blank, 72);
    let (device, phone) = halves(&words);
    let (mut device_damaged, mut phone_damaged) = (device, phone);
    device_damaged[7] = Some((words[7] + 1) % 2048);
    phone_damaged[25] = None;
    for (device, phone, corrected) in [(device, phone, 0), (device_damaged, phone_damaged, 1 << 7)]
    {
        let id = r.enter_card_with(Some(root.clone()), Some(remaining.clone()));
        r.phone_half(id, phone);
        r.device_half(device);
        let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
            panic!("an unlisted index still decodes against F")
        };
        assert_eq!(decoded.backup.share_image(), image);
        assert_eq!(findings.corrected, corrected);
        assert_eq!(findings.canonical, Some(true));
        assert!(!findings.threshold_differs);
    }

    // Two wrong words: the repair skips the unlisted index, so only a set that lists it helps.
    let mut device_two_wrong = device;
    device_two_wrong[3] = Some((words[3] + 1) % 2048);
    device_two_wrong[20] = Some((words[20] + 3) % 2048);
    let mut with_card_index = remaining.clone();
    with_card_index.push(card_index);
    let id = r.enter_card_with(Some(root.clone()), Some(remaining.clone()));
    r.phone_half(id, phone);
    r.device_half(device_two_wrong);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
    ));
    let id = r.enter_card_with(Some(root.clone()), Some(with_card_index));
    r.phone_half(id, phone);
    r.device_half(device_two_wrong);
    let Some(CardRestoreReport::Restored { decoded, findings }) = r.decode() else {
        panic!("repaired with the card's index listed")
    };
    assert_eq!(decoded.backup.share_image(), image);
    assert_eq!(findings.corrected, 1 << 3 | 1 << 20);

    // The app's "add to wallet": the share is consolidated into the wallet on the blank device.
    let sends = r.run.device(blank).card_restore_confirmed(decoded);
    r.handle(sends);
    let mut env = RestoreEnv::default();
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let phase = env.entered.pop().expect("the card was entered");
    let consolidate = run
        .coordinator
        .tell_device_to_consolidate_physical_backup(phase, asref, TEST_ENCRYPTION_KEY)
        .unwrap();
    run.extend(consolidate);
    run.run_until_finished(&mut env, &mut rng).unwrap();
    assert!(run
        .device(blank)
        .get_encrypted_share(asref, image.index)
        .is_some());
    assert!(run.coordinator.knows_about_share(blank, asref, image.index));
    assert!(indices(&run).contains(&card_index));
}

/// Owner B7, spec PK.8 (reverses B4): a card whose masks aren't the derived ones (a chip that
/// picked its own words 33–64 and hidden masks; its seal and proofs still hold, spec PK.2)
/// doesn't restore through the app, even though s·G is F's at its proof-bound index: with F or
/// without, it's "Couldn't read the card", the same report and the same `CouldNotRead` as words
/// that don't fit, and nothing to hold. Nor does a repair against F take it, nor does the device
/// that holds the share say `AlreadyHeld`. The offline repair tool may ignore the masks. A card
/// whose self-checking masks are honest but whose wallet-bound ones aren't restores cold, and
/// against F is `WrongWallet` (owner B11).
#[test]
fn a_card_with_dishonest_masks_doesnt_restore() {
    let mut run = key(2, 2, 56);
    let (asref, root, _cards) = cards(&mut run);
    let values = card_values(&mut run, asref, &root);
    let mut dishonest = values[0];
    let mut rng = StdRng::seed_from_u64(57);
    for slot in [0, 1, 3, 4]
        .into_iter()
        .chain(FIRST_PHONE_SLOT..FIRST_SHARE_SLOT)
    {
        dishonest[slot] = (rng.next_u32() % 2048) as u16;
    }
    let words = words_from_values(&dishonest, &phone_of(&dishonest));
    let blank = run.new_device(&mut StdRng::seed_from_u64(58));
    let mut r = Restorer::new(&mut run, blank, 59);
    let (device, phone) = halves(&words);
    for target in [Some(root.clone()), None] {
        let with_f = target.is_some();
        let id = r.enter_card(target);
        r.phone_half(id, phone);
        r.device_half(device);
        assert!(
            matches!(
                r.decode(),
                Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
            ),
            "F: {with_f}"
        );
        assert!(matches!(
            r.states.iter().rev().nth(1),
            Some(CardRestoreEvent::Outcome {
                result: RestoreResult::CouldNotRead,
            })
        ));
    }
    // Two wrong words on the device: past unique decoding, so the device repairs against F,
    // and s·G would be F's for the card's own codeword. The masks refuse it first.
    let mut two_wrong = device;
    for j in [3, 20] {
        two_wrong[j] = Some((words[j] + 1) % 2048);
    }
    let id = r.enter_card(Some(root.clone()));
    r.phone_half(id, phone);
    r.device_half(two_wrong);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
    ));
    // On the device that holds the share: "Couldn't read the card", not `AlreadyHeld`, even
    // for a `Decoded` that skipped the job's codec gate (a cold decode against F's entry).
    let holder = *run
        .coordinator
        .get_access_structure(asref)
        .unwrap()
        .device_to_share_indicies()
        .keys()
        .next()
        .unwrap();
    let mut r = Restorer::new(&mut run, holder, 60);
    for target in [Some(root.clone()), None] {
        let id = r.enter_card(target);
        r.phone_half(id, phone);
        r.device_half(device);
        assert!(matches!(
            r.decode(),
            Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
        ));
    }
    let id = r.enter_card(Some(root.clone()));
    r.phone_half(id, phone);
    r.device_half(device);
    let (_, generation, entries) = r.decodes.pop().expect("a decode");
    let decoded = codec::decode(&entries.0, None, None).unwrap();
    assert!(!decoded.masks.passed());
    let sends = r
        .run
        .device(holder)
        .card_restore_decoded(generation, RestoreOutcome::Decoded(Box::new(decoded)));
    r.handle(sends);
    assert!(matches!(
        r.reports.pop(),
        Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
    ));
    assert!(matches!(
        r.states.iter().rev().nth(1),
        Some(CardRestoreEvent::Outcome {
            result: RestoreResult::CouldNotRead,
        })
    ));
    let mut r = Restorer::new(&mut run, blank, 61);
    // Only the self-checking masks honest, and F's wallet-bound ones not: without F it
    // restores; with F it's a genuine card that isn't for this F (owner B11), and nothing
    // decoded is released.
    let mut wallet_liar = values[0];
    wallet_liar[FIRST_SHARE_SLOT - 1] ^= 1;
    let words = words_from_values(&wallet_liar, &phone_of(&wallet_liar));
    let (device, phone) = halves(&words);
    let id = r.enter_card(Some(root.clone()));
    r.phone_half(id, phone);
    r.device_half(device);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Failed(RestoreFailure::WrongWallet))
    ));
    assert!(matches!(
        r.states.iter().rev().nth(1),
        Some(CardRestoreEvent::Outcome {
            result: RestoreResult::WrongWallet,
        })
    ));
    let id = r.enter_card(None);
    r.phone_half(id, phone);
    r.device_half(device);
    assert!(matches!(
        r.decode(),
        Some(CardRestoreReport::Restored { .. })
    ));
}

// Audit regressions (2 Nov 2026): a lying chip's PASSED never skips this phone's final check
// (review/audit-lying-chip.md F1, F1b).

/// The chip's side of the link once it takes over: it sends `message` to the phone, and the
/// phone's sends are taken off the wire, so the honest firmware never sees them. Returns what
/// the phone sent to the device, or the phone's refusal of the frame.
fn chip_says(w: &mut World, message: DeviceCard) -> Result<Vec<CoordinatorCard>, String> {
    let sends = w
        .run
        .coordinator
        .recv_device_message(w.id, DeviceToCoordinatorMessage::Card(message))
        .map_err(|e| format!("{e:?}"))?;
    w.phone_out(sends);
    let mut to_device = vec![];
    loop {
        to_device.extend(w.wire.drain(..).filter_map(|frame| match frame {
            Frame::ToDevice(m) => Some(m),
            Frame::ToPhone(_) => None,
        }));
        match w.phone_actions.pop_front() {
            Some(action) => w.phone_act(action),
            None => break,
        }
    }
    Ok(to_device)
}

/// A lying chip that writes a wrong word for all 32 device words and never sends a bit proof,
/// run until this phone's own quiz on words 33–64 passed (`Checked`, h32 persisted). The chip
/// then takes over the link. Returns the `Status` it claims: PASSED, under the pinned seal.
fn liar_until_checked(w: &mut World) -> CardStatus {
    let truth = w.truth;
    w.device_lies = DEVICE_HALF
        .map(|j| (j as u8, (truth[j] + 1) % 2048))
        .collect();
    w.auto_final_check = false;
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::BitProofs { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump_until(|w| w.phone.checked);
    assert!(w.phone.checked);
    assert!(!w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap()
        .proven());
    chip_takes_over(w)
}

/// The chip takes over the link here: what's on the wire is dropped. Returns the `Status` it
/// claims: PASSED, under the pinned seal.
fn chip_takes_over(w: &mut World) -> CardStatus {
    let rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap()
        .clone();
    let pin = rec.pin.unwrap();
    w.filter = None;
    w.wire.clear();
    w.resend = None;
    w.phone_actions.clear();
    w.events.clear();
    CardStatus {
        share_image: w.share_image,
        format_id: frostsnap_core::coordinator::card::CARD_FORMAT,
        proof_version: proof::PROOF_VERSION,
        sealed: CardSealed {
            format_id: frostsnap_core::coordinator::card::CARD_FORMAT,
            h_c: pin.h_c,
            coin_commit: pin.coin_commit,
        },
        state: CardStateReport::Passed,
        check: CardCheckReport::Ended { passed: true },
        reason: None,
    }
}

/// What a chip's PASSED left on this phone: no later check, nothing that reads "Backup
/// checked", the record still NotChecked with no N, and the lying card doesn't restore.
fn assert_no_pass_from_the_chip(w: &World, sent: &[CoordinatorCard]) {
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, CoordinatorCard::Recheck { .. })),
        "{sent:?}"
    );
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
    let rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap();
    // Owner B8: once this phone sent N, the chip's PASSED is an abort with the suspect mark.
    match w.status() {
        CardBackupStatus::NotChecked => {}
        CardBackupStatus::Failed => assert!(rec.n.is_some() && w.suspect()),
        other => panic!("{other:?}"),
    }
    assert_eq!(rec.device_round, None);
    assert_ne!(rec.round, Some(Ok(())));
    let target = codec::Target::Share {
        image: w.share_image,
        threshold: 2,
    };
    assert!(codec::decode(&w.card, Some(&target), None).is_err());
}

/// Audit F1 (lying chip), inverted: once this phone's own quiz passed, the chip answers with
/// `Refused{NoSession}`, the protocol sends `Begin` again, and the chip claims PASSED. That is
/// another phone's check as far as this phone can tell (before N it can't tell a second phone
/// from a lie), so the phone shows `StartedOnAnotherPhone`: no later check, no quiz, no "Backup
/// checked". A `RecheckWords` the chip pushes anyway is refused, and its `RecheckDone` does
/// nothing.
#[test]
fn recheck_cannot_skip_the_final_check() {
    let mut w = World::new(2, 2, 77);
    let status = liar_until_checked(&mut w);
    let sent = chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        },
    )
    .unwrap();
    assert!(
        sent.iter().any(|m| matches!(m, CoordinatorCard::Begin(_))),
        "the app sends Begin again by itself: {sent:?}"
    );
    let sent = chip_says(&mut w, DeviceCard::Status(Box::new(status))).unwrap();
    // Owner B8: this phone sent N with the confirm, so it's an abort.
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Failed {
            device_suspect: true,
            ..
        }
    )));
    let phone_half: [u16; HALF] = w.truth[HALF..].try_into().unwrap();
    assert!(chip_says(
        &mut w,
        DeviceCard::RecheckWords {
            phone_half: CardWords32(phone_half),
            d_words: None,
        },
    )
    .is_err());
    chip_says(&mut w, DeviceCard::RecheckDone).unwrap();
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckDeviceDone)));
    assert_no_pass_from_the_chip(&w, &sent);
}

/// Audit F1, in the device round: this phone sent `FinalBegin` (the round is pending; owner B8:
/// N went out with the confirm) and the chip claims PASSED after a `NoSession`: an abort with
/// the suspect mark. No later check, and nothing reads as a pass.
#[test]
fn recheck_cannot_skip_the_device_round() {
    let mut w = World::new(2, 2, 79);
    w.auto_final_check = false;
    w.pump();
    assert!(w.phone.checked);
    // A reconnect, so the phone offers the final check again and takes it this time.
    w.cut_device(0);
    w.auto_final_check = true;
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { .. })));
    w.auto_final_check = false;
    let rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap();
    assert_eq!(rec.device_round, Some(DeviceRoundRecord::Pending));
    assert!(rec.n.is_some());
    let status = chip_takes_over(&mut w);
    chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        },
    )
    .unwrap();
    let sent = chip_says(&mut w, DeviceCard::Status(Box::new(status))).unwrap();
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Failed {
            device_suspect: true,
            ..
        }
    )));
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, CoordinatorCard::Recheck { .. })),
        "{sent:?}"
    );
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::RecheckPassed | CardEvent::Passed)));
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    let rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap();
    assert_ne!(rec.round, Some(Ok(())));
    assert_eq!(rec.device_passed, None);
}

/// Audit F1b, inverted: a `Status` no `Begin` asked for is refused and changes nothing, so a
/// chip can't push PASSED into a live flow.
#[test]
fn unsolicited_status_is_refused() {
    let mut w = World::new(3, 2, 78);
    let status = liar_until_checked(&mut w);
    let log = w.phone.log.len();
    assert!(chip_says(&mut w, DeviceCard::Status(Box::new(status.clone()))).is_err());
    assert!(w.events.is_empty(), "{:?}", w.events);
    assert_eq!(w.phone.log.len(), log);
    assert_no_pass_from_the_chip(&w, &[]);
    // A second `Status` for one `Begin` is refused too.
    chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        },
    )
    .unwrap();
    let sent = chip_says(&mut w, DeviceCard::Status(Box::new(status.clone()))).unwrap();
    assert!(chip_says(&mut w, DeviceCard::Status(Box::new(status))).is_err());
    assert_no_pass_from_the_chip(&w, &sent);
}

// Audit regressions (2 Nov 2026): a card restore releases nothing unless the masks pass, so the
// phone's words 33–64 and target are no oracle on s (review/audit-secrecy.md F1, F2; owner B7).
mod restore_oracle {
    use super::*;
    use frostsnap_core::message::DeviceRestoration;
    use schnorr_fun::frost::SecretShare;
    use schnorr_fun::fun::{g, marker::*, Scalar, G};

    // GF(2^11) mod x^11 + x^2 + 1 (0x805), as the card's code, written out here so the tamper
    // doesn't lean on the codec it attacks.
    fn gf_mul(a: u16, b: u16) -> u16 {
        let (mut a, mut b, mut r) = (a as u32, b as u32, 0u32);
        while b != 0 {
            if b & 1 != 0 {
                r ^= a;
            }
            b >>= 1;
            a <<= 1;
            if a & 0x800 != 0 {
                a ^= 0x805;
            }
        }
        r as u16
    }

    fn gf_inv(a: u16) -> u16 {
        let (mut r, mut base, mut e) = (1u16, a, 2046u32);
        while e > 0 {
            if e & 1 == 1 {
                r = gf_mul(r, base);
            }
            base = gf_mul(base, base);
            e >>= 1;
        }
        r
    }

    fn lagrange_eval(xs: &[u16], ys: &[u16], x: u16) -> u16 {
        let mut acc = 0;
        for k in 0..xs.len() {
            if ys[k] == 0 {
                continue;
            }
            let (mut num, mut den) = (1u16, 1u16);
            for m in 0..xs.len() {
                if m != k {
                    num = gf_mul(num, x ^ xs[m]);
                    den = gf_mul(den, xs[k] ^ xs[m]);
                }
            }
            acc ^= gf_mul(ys[k], gf_mul(num, gf_inv(den)));
        }
        acc
    }

    /// The malicious app's phone half: P′ = P + g(33..64), with g(1..32) = 0 and g(2019 + m) =
    /// `delta[m]`, so the device half and P′ make an exact codeword whose hidden values are the
    /// card's XOR `delta`.
    fn tamper(p: &[u16; HALF], delta: &[u16; layout::HIDDEN]) -> [u16; HALF] {
        let mut xs = vec![];
        let mut ys = vec![];
        for j in 0..HALF {
            xs.push(layout::word_point(j));
            ys.push(0);
        }
        for (m, &d) in delta.iter().enumerate() {
            xs.push(layout::hidden_point(m));
            ys.push(d);
        }
        core::array::from_fn(|k| p[k] ^ lagrange_eval(&xs, &ys, layout::word_point(HALF + k)))
    }

    /// The hidden-value delta that flips scalar bits `bits`, XORs `index_xor` into the index and
    /// `gamma` into the threshold value.
    fn delta_for(
        bits: impl IntoIterator<Item = usize>,
        index_xor: u8,
        gamma: u16,
    ) -> [u16; layout::HIDDEN] {
        let mut d = [0u16; layout::HIDDEN];
        for t in bits {
            let (slot, bit) = layout::scalar_bit_position(t);
            d[layout::hidden_index(slot).unwrap()] ^= 1 << bit;
        }
        // h0 = index (bits 10..3) ‖ s255 s254 s253, at x = 2024.
        d[layout::RANDOM_HIDDEN] ^= (index_xor as u16) << 3;
        d[THRESHOLD_SLOT] ^= gamma;
        d
    }

    fn two_pow(t: usize) -> Scalar<Public, Zero> {
        let mut b = [0u8; 32];
        b[31 - t / 8] = 1 << (t % 8);
        Scalar::from_bytes_mod_order(b)
    }

    /// A malicious app driving a blank device's card restore with raw frames: no honest
    /// coordinator in the loop.
    struct EvilPhone<'a> {
        run: &'a mut Run,
        id: DeviceId,
        entry: Option<CardRestorePhase>,
        decodes: Vec<(CardRestorePhase, u32, CardEntries)>,
        /// Every frame the device sent the phone, encoded, per decode.
        frames: Vec<Vec<u8>>,
        outcomes: Vec<RestoreResult>,
        reports: Vec<CardRestoreReport>,
        entered: Vec<ShareImage>,
    }

    impl<'a> EvilPhone<'a> {
        fn new(run: &'a mut Run, id: DeviceId) -> Self {
            Self {
                run,
                id,
                entry: None,
                decodes: vec![],
                frames: vec![],
                outcomes: vec![],
                reports: vec![],
                entered: vec![],
            }
        }

        fn handle(&mut self, sends: Vec<DeviceSend>) {
            for send in sends {
                match send {
                    DeviceSend::ToCoordinator(m) => {
                        self.frames.push(
                            bincode::encode_to_vec(&*m, bincode::config::standard()).unwrap(),
                        );
                        match *m {
                            DeviceToCoordinatorMessage::Card(DeviceCard::RestoreOutcome {
                                result,
                            }) => self.outcomes.push(result),
                            DeviceToCoordinatorMessage::Restoration(
                                DeviceRestoration::PhysicalEntered(e),
                            ) => self.entered.push(e.share_image),
                            _ => {}
                        }
                    }
                    DeviceSend::ToUser(m) => {
                        if let DeviceToUserMessage::Card(c) = *m {
                            match *c {
                                ToUserCard::RestoreEntry(p) => self.entry = Some(p),
                                ToUserCard::RestoreDecode {
                                    phase,
                                    generation,
                                    entries,
                                } => self.decodes.push((phase, generation, *entries)),
                                ToUserCard::RestoreReport(r) => self.reports.push(*r),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        fn enter_card(&mut self, target: Option<SharedKey>) -> EnterPhysicalId {
            let id = EnterPhysicalId::new(&mut StdRng::seed_from_u64(0xe711));
            let message =
                CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::EnterCard {
                    enter_physical_id: id,
                    target,
                    recorded_format: Some(proof::FORMAT_ID),
                    share_indices: None,
                });
            let sends = self
                .run
                .device(self.id)
                .recv_coordinator_message(message, &mut StdRng::seed_from_u64(0))
                .unwrap();
            self.handle(sends);
            id
        }

        /// The user types words 1–32 on the device and confirms: once per restore.
        fn user_enters_device_half(&mut self, words: [Option<u16>; HALF]) {
            let phase = self.entry.clone().unwrap();
            let sends = self
                .run
                .device(self.id)
                .card_restore_device_half(&phase, words);
            self.handle(sends);
        }

        /// A `CardPhoneHalf` frame (no user action), then the firmware's decode at T_dec.
        /// Returns the result and every frame the device sent the phone for it.
        fn query(&mut self, id: EnterPhysicalId, p: [u16; HALF]) -> (RestoreResult, Vec<Vec<u8>>) {
            let (result, frames, _) = self.query_counting(id, p);
            (result, frames)
        }

        /// [`query`](Self::query), also giving the work of each of the decode's
        /// `RestoreJob::step` slices: one per pass of the firmware's main loop, which the phone
        /// can time by its replies. The last is the slice the outcome came out of.
        fn query_counting(
            &mut self,
            id: EnterPhysicalId,
            p: [u16; HALF],
        ) -> (RestoreResult, Vec<Vec<u8>>, Vec<Work>) {
            let message =
                CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::CardPhoneHalf {
                    enter_physical_id: id,
                    words: CardHalfEntries(p.map(Some)),
                });
            self.frames.clear();
            let sends = self
                .run
                .device(self.id)
                .recv_coordinator_message(message, &mut StdRng::seed_from_u64(0))
                .unwrap();
            self.handle(sends);
            let (phase, generation, entries) =
                self.decodes.pop().expect("the device decodes by itself");
            let mut job = RestoreJob::new(
                &entries,
                phase.target.as_ref(),
                phase.recorded_format,
                phase.share_indices.as_deref(),
            );
            let (outcome, slices) = run_restore_job(&mut job);
            let sends = self
                .run
                .device(self.id)
                .card_restore_decoded(generation, outcome);
            self.handle(sends);
            let result = *self.outcomes.last().expect("a RestoreOutcome");
            (result, core::mem::take(&mut self.frames), slices)
        }
    }

    /// A card key's first share, its card's words, and the wallet's F.
    fn victim(seed: u64) -> (Run, SharedKey, SecretShare, [u16; WORDS]) {
        let mut run = key(2, 2, seed);
        let asref = run
            .coordinator
            .iter_access_structures()
            .next()
            .unwrap()
            .access_structure_ref();
        let root = run
            .coordinator
            .root_shared_key(asref, TEST_ENCRYPTION_KEY)
            .unwrap();
        let (&id, &index) = run
            .coordinator
            .get_access_structure(asref)
            .unwrap()
            .device_to_share_indicies()
            .iter()
            .next()
            .unwrap();
        let contrib = CoordShareDecryptionContrib::for_master_share(id, index, &root);
        let share = run
            .device(id)
            .get_encrypted_share(asref, index)
            .unwrap()
            .decrypt(asref, contrib, &mut TestDeviceKeyGen)
            .unwrap();
        let words = codec::encode(&share, &root).unwrap().words;
        (run, root, share, words)
    }

    fn split(words: &[u16; WORDS]) -> ([Option<u16>; HALF], [u16; HALF]) {
        (
            core::array::from_fn(|j| Some(words[j])),
            words[HALF..].try_into().unwrap(),
        )
    }

    /// The failure every tampered phone half gets: `CouldNotRead`, and on the device the
    /// "Couldn't read the card" page, with no report of the decoded card and so no hold.
    fn assert_couldnt_read(phone: &EvilPhone, result: RestoreResult) {
        assert_eq!(result, RestoreResult::CouldNotRead);
        assert!(matches!(
            phone.reports.last(),
            Some(CardRestoreReport::Failed(RestoreFailure::DontFit))
        ));
        assert!(phone.entered.is_empty(), "nothing was loaded");
    }

    /// Audit secrecy F1 and F2, the tamper itself: with words 1–32 as the user typed them,
    /// P′ = P + g decodes as a clean codeword (nothing corrected) to s ⊕ Δ at an index the phone
    /// picks, so only the masks can tell it from the card. The device loads nothing from it, in
    /// a cold restore or against F, and the honest P still restores in the same entry.
    #[test]
    fn restore_tampered_phone_half_loads_nothing() {
        let (mut run, root, share, words) = victim(901);
        let (device_half, p) = split(&words);
        let delta = delta_for([0, 5, 100, 255], 0, 0);
        let p2 = tamper(&p, &delta);
        let entries: codec::Entries = core::array::from_fn(|j| {
            if j < HALF {
                device_half[j]
            } else {
                Some(p2[j - HALF])
            }
        });
        let codeword = codec::decode_unique(&entries).unwrap();
        assert_eq!(codeword.corrected, 0, "the device sees a clean card");
        let honest = codec::decode_unique(&words.map(Some)).unwrap();
        let (h1, h2) = (honest.hidden_values(), codeword.hidden_values());
        for m in 0..layout::HIDDEN {
            assert_eq!(h1[m] ^ h2[m], delta[m]);
        }
        let decoded = codec::decode(&entries, None, None).unwrap();
        assert_ne!(decoded.share.share, share.share);
        assert!(!decoded.masks.passed(), "only the masks catch it");

        let blank = run.new_device(&mut StdRng::seed_from_u64(906));
        for target in [None, Some(root.clone())] {
            let mut phone = EvilPhone::new(&mut run, blank);
            let id = phone.enter_card(target);
            phone.user_enters_device_half(device_half);
            let (result, _) = phone.query(id, p2);
            assert_couldnt_read(&phone, result);
            // The honest words 33–64, same entry: restored, and the hold loads the card's share.
            let (result, _) = phone.query(id, p);
            assert_eq!(result, RestoreResult::Restored);
            let Some(CardRestoreReport::Restored { decoded, .. }) = phone.reports.pop() else {
                panic!("the honest card restores")
            };
            assert_eq!(decoded.backup.share_image(), share.share_image());
        }
    }

    /// Audit secrecy F2, inverted: a phone-chosen F″(x) = S_i + 2^a·x·G and phone halves that
    /// move the index to 2^b and flip scalar bit a + b made `RestoreOutcome` read bit a + b of
    /// s, one per decode, with no hold. Now every such decode gets the same frames, byte for
    /// byte, whatever the bit, the target (none, F, F″) or the threshold value, and the same
    /// "Couldn't read the card" on the device.
    #[test]
    fn restore_outcome_is_uniform_across_targets() {
        let (mut run, root, share, words) = victim(902);
        let i = u32::try_from(share.index).unwrap() as u8;
        let (device_half, p) = split(&words);
        let a = 100usize;
        let first = share.share_image().image.non_zero().unwrap();
        let slope = g!({ two_pow(a) } * G).normalize();
        let evil_f = SharedKey::from_non_zero_poly(first, [slope]);
        let blank = run.new_device(&mut StdRng::seed_from_u64(903));
        let mut seen = BTreeSet::new();
        for target in [Some(evil_f), None, Some(root)] {
            let mut phone = EvilPhone::new(&mut run, blank);
            let id = phone.enter_card(target);
            phone.user_enters_device_half(device_half);
            for b in 0..8 {
                for gamma in [0, 1 << 8] {
                    let index_xor = i ^ (1u8 << b);
                    let p2 = tamper(&p, &delta_for([a + b], index_xor, gamma));
                    let (result, frames) = phone.query(id, p2);
                    assert_couldnt_read(&phone, result);
                    seen.insert(frames);
                }
            }
        }
        assert_eq!(seen.len(), 1, "one answer for every query: {seen:?}");
    }

    /// The work of a decode's slices, checked to say nothing (owner R2-1): every slice but the
    /// finish does the same work, the finish comes at the same slice, and the outcome comes
    /// out at the same slice, T_dec. Returns the trace, for comparing across inputs.
    fn assert_constant_slices(slices: &[Work]) -> (Work, usize, Work, usize) {
        let search = slices[0];
        let odd: Vec<usize> = (0..slices.len()).filter(|&k| slices[k] != search).collect();
        assert_eq!(odd.len(), 1, "one slice is the finish: {slices:?}");
        let finish = odd[0];
        assert_eq!(search.checks, codec::SEARCH_SUBSETS_PER_SLICE as u64);
        assert_eq!(
            (search.rebuilds, search.spare_evals),
            (1, layout::PARITY as u64)
        );
        assert_eq!((search.reads, search.counters, search.images), (1, 256, 0));
        assert_eq!(
            (
                slices[finish].reads,
                slices[finish].counters,
                slices[finish].checks
            ),
            (1, 256, 0)
        );
        (search, finish, slices[finish], slices.len())
    }

    /// secfix-attack B1 and review RA-2 (owner R2-1), inverted. B1: the codec checked s′·G
    /// against the phone's F″ before the masks, so bit a + b = 0 ended the job in one slice
    /// and bit 1 ran the whole repair. RA-2: with the masks first, a repair slice still ended
    /// early at the first s·G image and the job stopped once the decode was done, so reply
    /// latency showed where the planted error set was, which depends on h0 and the user's
    /// typos. Now every slice until T_dec does the same work and the outcome comes out at the
    /// same slice, whatever the words: the honest card with or without typos (a unique decode,
    /// a repair, a repair with an unreadable word), another wallet's card, phone halves
    /// tampered by a bit of s, words that don't fit, too many unreadable words, against F″,
    /// the real F, or none.
    #[test]
    fn restore_work_doesnt_depend_on_s() {
        let (mut run, root, share, words) = victim(902);
        let (_, _, _, other_words) = victim(920);
        let i = u32::try_from(share.index).unwrap() as u8;
        let (device_half, p) = split(&words);
        let s_bytes = share.share.to_bytes();
        let bit_of_s = |t: usize| (s_bytes[31 - t / 8] >> (t % 8)) & 1;
        let a = 100usize;
        let first = share.share_image().image.non_zero().unwrap();
        let slope = g!({ two_pow(a) } * G).normalize();
        let evil_f = SharedKey::from_non_zero_poly(first, [slope]);
        let blank = run.new_device(&mut StdRng::seed_from_u64(903));
        let bits: Vec<usize> = (0..4).collect();
        let truths: BTreeSet<u8> = bits.iter().map(|b| bit_of_s(a + b)).collect();
        assert_eq!(truths.len(), 2, "both values of the bit are tried");

        let typo = |half: &mut [Option<u16>; HALF], k: usize| {
            half[k] = half[k].map(|w| (w + 1) % 2048);
        };
        let mut one_typo = device_half;
        typo(&mut one_typo, 7);
        let mut two_typos = device_half;
        typo(&mut two_typos, 3);
        typo(&mut two_typos, 29);
        let mut unreadable = device_half;
        typo(&mut unreadable, 30);
        typo(&mut unreadable, 31);
        unreadable[0] = None;
        let mut too_many = device_half;
        too_many[..4].fill(None);
        let (other_device, other_p) = split(&other_words);
        let garbage: [u16; HALF] = core::array::from_fn(|k| (k as u16 * 97 + 5) % 2048);

        for target in [Some(evil_f.clone()), Some(root.clone()), None] {
            let against_root = target.as_ref() == Some(&root);
            // Words 1–32, words 33–64, and the result when it's known.
            type Case = ([Option<u16>; HALF], [u16; HALF], Option<RestoreResult>);
            let mut cases: Vec<Case> = vec![
                (
                    device_half,
                    p,
                    against_root.then_some(RestoreResult::Restored),
                ),
                (one_typo, p, against_root.then_some(RestoreResult::Restored)),
                (
                    two_typos,
                    p,
                    against_root.then_some(RestoreResult::Restored),
                ),
                (unreadable, p, None),
                (too_many, p, Some(RestoreResult::CouldNotRead)),
                (device_half, garbage, Some(RestoreResult::CouldNotRead)),
                (other_device, other_p, None),
            ];
            for b in &bits {
                let p2 = tamper(&p, &delta_for([a + b], i ^ (1u8 << b), 0));
                cases.push((device_half, p2, Some(RestoreResult::CouldNotRead)));
            }
            let mut traces = vec![];
            for (device, phone, expected) in cases {
                let mut ph = EvilPhone::new(&mut run, blank);
                let id = ph.enter_card(target.clone());
                ph.user_enters_device_half(device);
                let (result, _, slices) = ph.query_counting(id, phone);
                if let Some(expected) = expected {
                    assert_eq!(result, expected, "F: {}", target.is_some());
                }
                traces.push(assert_constant_slices(&slices));
            }
            assert!(
                traces.windows(2).all(|w| w[0] == w[1]),
                "the work says nothing: {traces:?}"
            );
        }
    }

    /// The repairs the constant-work decode does in time (owner R2-1): two wrong words, and two
    /// wrong words with an unreadable one, with the wallet's index set as the app sends it
    /// (build plan A6), restore the card's share in a simulated T_dec.
    #[test]
    fn the_constant_work_decode_repairs_in_time() {
        let (_run, root, share, words) = victim(911);
        let mut entries: [Option<u16>; WORDS] = words.map(Some);
        for j in [5, 62] {
            entries[j] = entries[j].map(|w| (w + 3) % 2048);
        }
        for unreadable in [false, true] {
            let mut entries = entries;
            if unreadable {
                entries[20] = None;
            }
            let mut job = RestoreJob::new(&CardEntries(entries), Some(&root), None, Some(&[1, 2]));
            let (outcome, slices) = run_restore_job(&mut job);
            let RestoreOutcome::Decoded(decoded) = &outcome else {
                panic!("repaired, unreadable word: {unreadable}")
            };
            assert_eq!(decoded.share, share);
            assert_constant_slices(&slices);
        }
    }

    /// Review B15 secrecy S-2: the phone could starve the decode of slices (a reply waiting in
    /// the outbox skips a poll's slice) and let only k run before T_dec − 1 s; the finish ran
    /// on the wall clock, so Restored or "couldn't read" said whether the search ended within k
    /// slices. Now the search always runs [`RESTORE_SEARCH_SLICES`] slices before the finish,
    /// so a starved decode does the same slices, in the same order, with the same outcome; it
    /// only comes out later.
    #[test]
    fn a_starved_decode_does_the_same_search() {
        let (_run, root, share, words) = victim(912);
        let mut entries: [Option<u16>; WORDS] = words.map(Some);
        // Late in the search's order, so it takes more than a few slices to reach them.
        for j in [50, 60] {
            entries[j] = entries[j].map(|w| (w + 3) % 2048);
        }
        entries[57] = None;
        let job = || RestoreJob::new(&CardEntries(entries), Some(&root), None, Some(&[1, 2]));
        let (outcome, honest) = run_restore_job(&mut job());
        let RestoreOutcome::Decoded(decoded) = &outcome else {
            panic!("the honest schedule repairs it")
        };
        assert_eq!(decoded.share, share);
        let mut probe = job();
        let mut needed = 0;
        while !probe.search_done() {
            probe.step(0);
            needed += 1;
        }
        assert!(needed > 3, "the search needs more than 3 slices: {needed}");

        // 3 slices, then nothing until 9 s, or until well past T_dec; then a slice per poll.
        for starved_until in [9_000, 3 * RESTORE_DECODE_MS] {
            let mut job = job();
            let mut slices = vec![];
            let mut elapsed = 0;
            let outcome = loop {
                let before = job.work();
                let outcome = job.step(elapsed);
                slices.push(job.work().since(&before));
                if let Some(outcome) = outcome {
                    break outcome;
                }
                elapsed = if slices.len() == 3 {
                    starved_until
                } else {
                    elapsed + SIM_SLICE_MS
                };
            };
            let RestoreOutcome::Decoded(decoded) = &outcome else {
                panic!("starved until {starved_until} ms: the search still ran in full")
            };
            assert_eq!(decoded.share, share);
            let finish = RESTORE_SEARCH_SLICES as usize;
            assert_eq!(
                slices[..=finish],
                honest[..=finish],
                "the same search slices and finish"
            );
            assert!(slices.len() > finish);
            assert_constant_slices(&slices);
        }
    }

    /// The escalating restore-failure flow (owner, 2 Nov 2026) is the app's own count: the
    /// device keeps none that reaches the wire. Successive failures in one entry, for different
    /// reasons (words with typos, the same again, a tampered phone half, words from nowhere), and
    /// the first failure of a new entry, cold or against F, all get the same frames, byte for
    /// byte, and the same "Couldn't read the card" on the device. So the first, second and
    /// third failure look alike to the phone, and its escalation can't be steered by the device
    /// or read anything from it.
    #[test]
    fn successive_failures_send_the_same_frames() {
        let (mut run, root, _share, words) = victim(907);
        let (device_half, p) = split(&words);
        // Past any repair: more wrong words than a decode can correct.
        let mut typos = p;
        for (k, flip) in [(3, 1), (9, 0x40), (20, 0x81), (27, 0x200), (31, 0x11)] {
            typos[k] ^= flip;
        }
        let tampered = tamper(&p, &delta_for([7, 77], 0, 0));
        let mut rng = StdRng::seed_from_u64(908);
        let nowhere: [u16; HALF] = core::array::from_fn(|_| (rng.next_u32() % 2048) as u16);
        let blank = run.new_device(&mut StdRng::seed_from_u64(909));
        let mut seen = BTreeSet::new();
        let mut failures = 0;
        for target in [None, None, Some(root)] {
            let mut phone = EvilPhone::new(&mut run, blank);
            let id = phone.enter_card(target);
            phone.user_enters_device_half(device_half);
            for p2 in [typos, typos, tampered, nowhere] {
                let (result, frames) = phone.query(id, p2);
                assert_couldnt_read(&phone, result);
                seen.insert(frames);
                failures += 1;
            }
            assert_eq!(phone.reports.len(), 4, "one report per failure");
        }
        assert_eq!(failures, 12);
        assert_eq!(seen.len(), 1, "every failure looks alike: {seen:?}");
    }

    /// Audit secrecy F1, inverted: in a cold restore the phone flipped a k-bit window of s,
    /// steered the unbound threshold field so no threshold page showed, and after the user's
    /// one hold read s′·G from `PhysicalEntered`, a window of s for 2^(k/2) work. Now none of
    /// those decodes gets a report to hold, nothing is loaded and no share image is sent, and
    /// the threshold steering has nothing to read.
    #[test]
    fn cold_restore_releases_no_image_on_mask_failure() {
        let (mut run, _root, share, words) = victim(904);
        let (device_half, p) = split(&words);
        let window = 64..64 + 28;
        let blank = run.new_device(&mut StdRng::seed_from_u64(905));
        let mut phone = EvilPhone::new(&mut run, blank);
        let id = phone.enter_card(None);
        phone.user_enters_device_half(device_half);
        let mut seen = BTreeSet::new();
        for top in 0..8u16 {
            let (result, frames) =
                phone.query(id, tamper(&p, &delta_for(window.clone(), 0, top << 8)));
            assert_couldnt_read(&phone, result);
            seen.insert(frames);
        }
        assert_eq!(
            seen.len(),
            1,
            "the threshold field changes nothing the phone hears"
        );
        assert!(phone
            .reports
            .iter()
            .all(|r| matches!(r, CardRestoreReport::Failed(RestoreFailure::DontFit))));
        assert!(phone.entered.is_empty(), "no share image");
        // The honest words: restored, and the hold sends the card's own image.
        let (result, _) = phone.query(id, p);
        assert_eq!(result, RestoreResult::Restored);
        let Some(CardRestoreReport::Restored { decoded, findings }) = phone.reports.pop() else {
            panic!("the honest card restores")
        };
        assert_eq!(findings.threshold, Some(2));
        let sends = phone.run.device(blank).card_restore_confirmed(decoded);
        phone.handle(sends);
        assert_eq!(phone.entered, [share.share_image()]);
    }

    /// A card whose self-checking masks aren't the derived ones: a lying writer chip's, or a
    /// card in a format this firmware doesn't know (spec PK.8). Its words fit, its share is the
    /// card's own, and only the masks catch it.
    fn dishonest_self_masks(share: &SecretShare, root: &SharedKey, seed: u64) -> [u16; WORDS] {
        let mut values = codec::encode(share, root).unwrap().values;
        let mut rng = StdRng::seed_from_u64(seed);
        for slot in [0, 1, 3, 4]
            .into_iter()
            .chain(FIRST_PHONE_SLOT..FIRST_SHARE_SLOT)
        {
            values[slot] = (rng.next_u32() % 2048) as u16;
        }
        words_from_values(&values, &phone_of(&values))
    }

    /// Owner B11 (spec PK-O8): `WrongWallet` only for a genuine card, that is one that decodes
    /// uniquely with its self-checking masks passing, whose s·G isn't the phone's F at its index.
    /// Another wallet's card against this wallet's F, clean or with a word to correct, is
    /// `WrongWallet`, with no report to hold and nothing loaded; cold, or against its own F, it
    /// restores. Past unique decoding the device repairs only against F, so the same card with
    /// two wrong words is "couldn't read". A tampered phone half of it fails its self masks, and
    /// that's "couldn't read" too, whatever the F.
    #[test]
    fn wrong_wallet_only_for_a_genuine_card() {
        let (mut run, root, _share, _words) = victim(911);
        let (_, other_root, other_share, other_words) = victim(912);
        let (device_half, p) = split(&other_words);
        let blank = run.new_device(&mut StdRng::seed_from_u64(913));

        let mut phone = EvilPhone::new(&mut run, blank);
        let id = phone.enter_card(Some(root.clone()));
        phone.user_enters_device_half(device_half);
        let (result, _) = phone.query(id, p);
        assert_eq!(result, RestoreResult::WrongWallet);
        assert!(matches!(
            phone.reports.last(),
            Some(CardRestoreReport::Failed(RestoreFailure::WrongWallet))
        ));
        // One word to correct, still unique decoding.
        let mut one_wrong = p;
        one_wrong[7] ^= 0x15;
        let (result, _) = phone.query(id, one_wrong);
        assert_eq!(result, RestoreResult::WrongWallet);
        // Two wrong words: past unique decoding, a repair against this F, which can't find
        // another wallet's card.
        let mut two_wrong = one_wrong;
        two_wrong[20] ^= 0x2a;
        let (result, _) = phone.query(id, two_wrong);
        assert_couldnt_read(&phone, result);
        // Its phone half tampered: its self masks fail.
        let tampered = tamper(&p, &delta_for([3, 140], 0, 0));
        let (result, _) = phone.query(id, tampered);
        assert_couldnt_read(&phone, result);
        assert!(phone.entered.is_empty(), "nothing was loaded");

        for target in [None, Some(other_root)] {
            let mut phone = EvilPhone::new(&mut run, blank);
            let id = phone.enter_card(target);
            phone.user_enters_device_half(device_half);
            let (result, _) = phone.query(id, p);
            assert_eq!(result, RestoreResult::Restored);
            let Some(CardRestoreReport::Restored { decoded, .. }) = phone.reports.pop() else {
                panic!("its own card restores")
            };
            assert_eq!(decoded.backup.share_image(), other_share.share_image());
        }
    }

    /// Owner B11 keeps B7's single failure for every self-mask failure: all of them get the same
    /// frames, byte for byte, as words that don't fit at all, cold and against F (the real one
    /// or one the phone chose). The cases: phone halves tampered to flip scalar bits, the index
    /// or the threshold value; a card with non-derived self masks (a lying writer chip, or a
    /// format this firmware doesn't know), clean and with a word for a repair to find; another
    /// wallet's card with its phone half tampered; and typos past any repair. Only a genuine
    /// card for another wallet gets other frames (`WrongWallet`).
    #[test]
    fn every_self_mask_failure_sends_the_same_frames() {
        let (mut run, root, share, words) = victim(914);
        let i = u32::try_from(share.index).unwrap() as u8;
        let (_, _, _, other_words) = victim(915);
        let first = share.share_image().image.non_zero().unwrap();
        let slope = g!({ two_pow(100) } * G).normalize();
        let evil_f = SharedKey::from_non_zero_poly(first, [slope]);
        let liar = dishonest_self_masks(&share, &root, 916);
        let mut liar_one_wrong = liar;
        liar_one_wrong[5] ^= 0x31;
        let blank = run.new_device(&mut StdRng::seed_from_u64(917));
        let mut rng = StdRng::seed_from_u64(918);
        let nowhere: [u16; HALF] = core::array::from_fn(|_| (rng.next_u32() % 2048) as u16);

        let (device_half, p) = split(&words);
        let (liar_device, liar_p) = split(&liar);
        let (liar_wrong_device, _) = split(&liar_one_wrong);
        let (other_device, other_p) = split(&other_words);
        let mut typos = p;
        for (k, flip) in [(2, 3), (8, 0x40), (19, 0x81), (26, 0x200)] {
            typos[k] ^= flip;
        }
        // (what fails, words 1–32 the user enters, words 33–64 the phone sends)
        type Case = (&'static str, [Option<u16>; HALF], [u16; HALF]);
        let cases: Vec<Case> = vec![
            ("words from nowhere", device_half, nowhere),
            (
                "scalar bits",
                device_half,
                tamper(&p, &delta_for([0, 77, 255], 0, 0)),
            ),
            (
                "index",
                device_half,
                tamper(&p, &delta_for([], i ^ 0x10, 0)),
            ),
            (
                "threshold value",
                device_half,
                tamper(&p, &delta_for([], 0, 1 << 8)),
            ),
            ("non-derived self masks", liar_device, liar_p),
            (
                "non-derived self masks, a word to repair",
                liar_wrong_device,
                liar_p,
            ),
            (
                "another wallet's card, tampered",
                other_device,
                tamper(&other_p, &delta_for([9], 0, 0)),
            ),
            ("typos past any repair", device_half, typos),
        ];
        let mut seen = BTreeSet::new();
        for target in [None, Some(root.clone()), Some(evil_f)] {
            for (name, device, phone_half) in &cases {
                let mut phone = EvilPhone::new(&mut run, blank);
                let id = phone.enter_card(target.clone());
                phone.user_enters_device_half(*device);
                let (result, frames) = phone.query(id, *phone_half);
                assert_eq!(
                    result,
                    RestoreResult::CouldNotRead,
                    "{name}, F: {}",
                    target.is_some()
                );
                assert_couldnt_read(&phone, result);
                seen.insert(frames);
            }
        }
        assert_eq!(
            seen.len(),
            1,
            "every self-mask failure looks alike: {seen:?}"
        );

        // A genuine card for another wallet is the one other answer.
        let mut phone = EvilPhone::new(&mut run, blank);
        let id = phone.enter_card(Some(root));
        phone.user_enters_device_half(other_device);
        let (result, frames) = phone.query(id, other_p);
        assert_eq!(result, RestoreResult::WrongWallet);
        assert!(!seen.contains(&frames));
    }

    /// Owner B11 and secfix-attack B1 together: `WrongWallet` adds no work channel. Another
    /// wallet's genuine card takes the same decode slices against this F (`WrongWallet`) as
    /// against its own (`Restored`). And a tampered phone half, whose self masks fail so the
    /// repair runs, takes the same slices too (owner R2-1).
    #[test]
    fn wrong_wallet_work_matches_restored() {
        let (mut run, root, _share, _words) = victim(919);
        let (_, other_root, _other_share, other_words) = victim(920);
        let (device_half, p) = split(&other_words);
        let blank = run.new_device(&mut StdRng::seed_from_u64(921));
        let mut slices = vec![];
        for (target, expected) in [
            (root.clone(), RestoreResult::WrongWallet),
            (other_root, RestoreResult::Restored),
        ] {
            let mut phone = EvilPhone::new(&mut run, blank);
            let id = phone.enter_card(Some(target));
            phone.user_enters_device_half(device_half);
            let (result, _, n) = phone.query_counting(id, p);
            assert_eq!(result, expected);
            slices.push(n);
        }
        assert_eq!(slices[0], slices[1], "{slices:?}");
        let tampered = tamper(&p, &delta_for([1, 2], 0, 0));
        let mut phone = EvilPhone::new(&mut run, blank);
        let id = phone.enter_card(Some(root));
        phone.user_enters_device_half(device_half);
        let (result, _, n) = phone.query_counting(id, tampered);
        assert_couldnt_read(&phone, result);
        assert_eq!(
            n, slices[0],
            "a self-mask failure's repair costs the same work"
        );
    }
}

// Owner B8 (2 Nov 2026): the phone's N goes out right after "I've written words 1–32", so the
// device computes the openings of D while words 33–64 are written.

/// Owner B8: a chip that took N (so knew D) and then reports a failed device round is not
/// blamed on the phone: FAILED, the device suspect, after N.
#[test]
fn a_chip_failing_the_device_round_after_n_is_suspect() {
    let mut w = World::new(2, 2, 81);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::DeviceRound { pass: true }) => {
            Tamper::Pass(Frame::ToPhone(DeviceCard::DeviceRound { pass: false }))
        }
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert!(w.record().unwrap().n_tag.is_some(), "the chip took N");
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(
        w.events.iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                reason: CardFailReason::DeviceRoundFailed,
                device_suspect: true,
                after_n: true,
            }
        )),
        "{:?}",
        w.events
    );
    // The phone told the device the check is over.
    assert!(w.sent_to_device(|m| matches!(m, CoordinatorCard::PhoneRound { pass: false, .. })));
    assert!(matches!(w.state(), Some(CardState::Failed(_))));
}

/// Owner B8: an honest unplug or app restart while words 33–64 are written resumes with the
/// same N and D, drawn once, and the card passes.
#[test]
fn a_resume_during_the_phone_half_keeps_n_and_d() {
    for app in [false, true] {
        let mut w = World::new(2, 2, 83);
        w.pump_until(|w| {
            w.record().is_some_and(|rec| rec.n_tag.is_some())
                && w.events
                    .iter()
                    .any(|e| matches!(e, CardEvent::PhonePage { page: 3, .. }))
        });
        let n = w.phone.n.expect("N persisted");
        let d = w.record().unwrap().d.expect("D persisted");
        if app {
            w.cut_app(true);
        } else {
            w.cut_device(0);
        }
        w.pump();
        w.assert_passed(&format!("app restart {app}"));
        assert_eq!(w.phone.n, Some(n), "the same N");
        assert_eq!(w.record().unwrap().d, Some(d), "the same D");
        assert_eq!(w.phone.d, Some(d));
        let drawn = w
            .phone
            .log
            .iter()
            .filter(|m| matches!(m, CoordCardMutation::CheckStarted { .. }))
            .count();
        assert_eq!(drawn, 1, "N drawn once");
        assert!(!w.suspect());
    }
}

/// Owner B8: the device takes N only in PROVING, after its own quiz on words 1–32 passed and
/// the phone's `DeviceHalfWritten`; earlier it refuses and remembers nothing. In PROVING it
/// takes no N but the first.
#[test]
fn n_is_refused_before_the_confirm() {
    let n = [7u8; 32];
    // The device's quiz is up: not passed yet.
    let mut w = World::new(2, 2, 85);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::SelfCheck { .. })));
    let replies = w.device_replies(CoordinatorCard::OpenStart { n });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::WrongState
            }]
        ),
        "{replies:?}"
    );
    assert_eq!(w.record().unwrap().n_tag, None);

    // The quiz passed (CARD_CHECKED) but no confirm: `Busy` (the phone sends its confirm again)
    // and nothing is taken.
    let mut w = World::new(2, 2, 86);
    w.auto_confirm = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::CardChecked));
    let replies = w.device_replies(CoordinatorCard::OpenStart { n });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::Busy
            }]
        ),
        "{replies:?}"
    );
    assert_eq!(w.record().unwrap().n_tag, None);
    assert_eq!(w.state(), Some(CardState::CardChecked));

    // PROVING with the phone's N taken: another N is refused, and the record keeps the first.
    let mut w = World::new(2, 2, 87);
    w.auto_final_check = false;
    w.pump();
    let n_tag = w.record().unwrap().n_tag.expect("the phone's N");
    let replies = w.device_replies(CoordinatorCard::OpenStart { n });
    assert!(
        matches!(
            replies.as_slice(),
            [DeviceCard::Refused {
                reason: CardRefusal::AlreadyUsed
            }]
        ),
        "{replies:?}"
    );
    assert_eq!(w.record().unwrap().n_tag, Some(n_tag));
}

// Owner B9 (2 Nov 2026): a disconnect during the check, and one phone per check.

/// Whether the phone's user was asked about an interruption since `from`.
fn asked(w: &World, from: usize) -> bool {
    w.events[from..]
        .iter()
        .any(|e| matches!(e, CardEvent::Interrupted { .. }))
}

/// A named point in the flow: whether the flow has reached it.
type NamedPoint = (&'static str, fn(&mut World) -> bool);

/// The points after N where the D check runs: words 33–64 being written (the device proving
/// and opening), the device round, and the phone round (the device at its gate).
fn after_n_points() -> [NamedPoint; 3] {
    [
        ("words 33–64", |w| {
            w.record().is_some_and(|rec| rec.n_tag.is_some())
                && w.events
                    .iter()
                    .any(|e| matches!(e, CardEvent::PhonePage { page: 4, .. }))
        }),
        ("the device round", |w| {
            matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { k: 1, .. }))
        }),
        ("the phone round", |w| {
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::Question { k: 1, .. }))
        }),
    ]
}

/// Owner B9: an unplug after N, wherever the D check is (words 33–64, the device round, the
/// phone round), is persisted as an interruption and asks the user whether they caused it.
/// Nothing goes to the device until they answer, not even `Begin`, and no device frame is taken.
/// "Yes, it was me": the check resumes on the same N and D, and passes.
#[test]
fn a_disconnect_after_n_asks_the_user() {
    for (i, (point, reached)) in after_n_points().into_iter().enumerate() {
        let mut w = World::new(2, 2, 131 + i as u64);
        w.pump_until(reached);
        let n = w.phone.n.expect("N is out");
        let d = w.record().unwrap().d.expect("D is drawn");
        w.interruption_answer = None;
        let (events, frames) = (w.events.len(), w.frames.len());
        w.cut_device(0);
        w.pump();
        assert!(asked(&w, events), "{point}");
        assert!(
            w.phone
                .has(|m| matches!(m, CoordCardMutation::CheckInterrupted { .. })),
            "{point}: persisted"
        );
        assert!(w.run.coordinator.card_interruption_pending(w.id));
        assert_eq!(
            w.frames.len(),
            frames,
            "{point}: nothing goes to the device"
        );
        // A device frame isn't taken meanwhile, nor any other action.
        let status = w.run.coordinator.recv_device_message(
            w.id,
            DeviceToCoordinatorMessage::Card(DeviceCard::GateShown),
        );
        assert!(status.is_err(), "{point}");
        assert!(w
            .run
            .coordinator
            .card_final_begin(w.id, &mut StdRng::seed_from_u64(1))
            .is_err());

        w.phone_act(PhoneAction::AnswerInterruption(true));
        assert!(w.events.iter().any(|e| matches!(e, CardEvent::Resuming)));
        w.pump();
        w.assert_passed(point);
        assert_eq!(w.phone.n, Some(n), "{point}: the same N");
        assert_eq!(w.phone.d, Some(d), "{point}: the same D");
        assert_eq!(
            w.phone
                .log
                .iter()
                .filter(|m| matches!(m, CoordCardMutation::CheckStarted { .. }))
                .count(),
            1,
            "{point}: N drawn once"
        );
    }
}

/// Owner R2-2: "Yes, it was me" resumes one check at most 3 times. Unplugs and app restarts
/// all count, and the count is in the log, so a restart doesn't reset it. The 4th interruption
/// still asks; "yes" then ends the check: an abort with the device suspect and its own reason,
/// and no 4th resume is persisted. "No" is B9's abort, as always. This closes a chip (or a
/// phantom device on its port) that keeps interrupting the check, and the replug-forever stall
/// (review F1(4)).
#[test]
fn a_fourth_yes_ends_the_check_with_the_device_suspect() {
    let resumed = |w: &World| {
        w.phone
            .log
            .iter()
            .filter(|m| matches!(m, CoordCardMutation::InterruptionAnswered { .. }))
            .count()
    };
    for (answer, reason) in [
        (true, CardFailReason::TooManyInterruptions),
        (false, CardFailReason::Interrupted),
    ] {
        let mut w = World::new(2, 2, 185);
        let n = {
            w.pump_until(after_n_points()[0].1);
            w.phone.n.expect("N is out")
        };
        // Three interruptions, each answered "yes": an unplug, an app restart, an unplug.
        let points = after_n_points();
        for (k, (point, _)) in points.iter().enumerate() {
            let events = w.events.len();
            if k == 1 {
                w.cut_app(true);
            } else {
                w.cut_device(0);
            }
            let next: fn(&mut World) -> bool = match points.get(k + 1) {
                Some((_, reached)) => *reached,
                None => |w| {
                    w.events
                        .iter()
                        .any(|e| matches!(e, CardEvent::Question { k: 3, .. }))
                },
            };
            w.pump_until(next);
            assert!(asked(&w, events) || k == 1, "{point}: asked");
            // Review B15 protocol F2: the question says how many resumes are left, so the
            // page can warn before the last one.
            if k != 1 {
                let left = w.events[events..].iter().find_map(|e| match e {
                    CardEvent::Interrupted { resumes_left } => Some(*resumes_left),
                    _ => None,
                });
                assert_eq!(left, Some(MAX_INTERRUPTION_RESUMES - k as u8), "{point}");
            }
            assert_eq!(resumed(&w), k + 1, "{point}: resumed");
            assert_eq!(w.status(), CardBackupStatus::NotChecked, "{point}");
            assert!(!w.suspect(), "{point}");
        }
        assert_eq!(w.phone.n, Some(n), "the same N throughout");
        // The 4th, after an app restart: the count came back from the log.
        w.interruption_answer = None;
        w.cut_app(true);
        w.pump();
        assert!(w.run.coordinator.card_interruption_pending(w.id));
        assert_eq!(w.run.coordinator.card_resumes_left(w.id), 0);
        w.interruption_answer = Some(answer);
        w.phone_act(PhoneAction::AnswerInterruption(answer));
        w.pump();
        w.assert_aborted(reason);
        assert_eq!(
            resumed(&w),
            MAX_INTERRUPTION_RESUMES as usize,
            "no 4th resume"
        );
        assert!(!w.run.coordinator.card_interruption_pending(w.id));
    }
}

/// Owner B9: "No, I didn't" ends the check: an abort, the device marked suspect, with its own
/// reason, and the device fails the card once it hears of it.
#[test]
fn no_i_didnt_marks_the_device_suspect() {
    for (i, (point, reached)) in after_n_points().into_iter().enumerate() {
        let mut w = World::new(2, 2, 141 + i as u64);
        w.pump_until(reached);
        w.interruption_answer = Some(false);
        w.auto_continue = false;
        w.cut_device(0);
        w.pump();
        assert_eq!(w.status(), CardBackupStatus::Failed, "{point}");
        assert!(w.suspect(), "{point}");
        assert!(
            w.events.iter().any(|e| matches!(
                e,
                CardEvent::Failed {
                    reason: CardFailReason::Interrupted,
                    device_suspect: true,
                    ..
                }
            )),
            "{point}: {:?}",
            w.events
        );
        assert!(!w.run.coordinator.card_interruption_pending(w.id));
        // The device hears of it with the failed round.
        w.phone_actions.push_back(PhoneAction::Begin);
        w.pump();
        assert_eq!(
            w.state(),
            Some(CardState::Failed(VerdictReason::PhoneRoundFailed)),
            "{point}"
        );
    }
}

/// Owner B9: before N a disconnect resumes as before, with no question and nothing recorded:
/// D isn't fixed yet, so there's nothing to re-roll. Nor after the phone round's result, when
/// only the device's verdict is left.
#[test]
fn a_disconnect_before_n_asks_nothing() {
    type Point = fn(&mut World) -> bool;
    let points: [(&str, Point); 3] = [
        ("words 1–32 on the device", |w| {
            matches!(w.screen, Some(CardScreen::DeviceHalf { .. }))
        }),
        ("the device's quiz", |w| {
            matches!(w.screen, Some(CardScreen::SelfCheck { .. }))
        }),
        ("the confirm of words 1–32", |w| {
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::ConfirmDeviceHalf))
        }),
    ];
    for (point, reached) in points {
        let mut w = World::new(2, 2, 151);
        w.auto_confirm = false;
        w.pump_until(reached);
        assert!(w.phone.n.is_none(), "{point}");
        let events = w.events.len();
        w.cut_device(0);
        w.auto_confirm = true;
        w.pump();
        assert!(!asked(&w, events), "{point}");
        w.assert_passed(point);
        assert!(!w
            .phone
            .has(|m| matches!(m, CoordCardMutation::CheckInterrupted { .. })));
    }

    // The phone round passed, and the verdict is lost to an unplug: collected with no question.
    let mut w = World::new(2, 2, 152);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::Verdict { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::NotChecked);
    w.filter = None;
    let events = w.events.len();
    w.cut_device(0);
    w.pump();
    assert!(!asked(&w, events));
    w.assert_passed("the verdict after an unplug");
}

/// Owner B9: an app closed while the D check runs asks the same question when the flow opens
/// again (the record holds no unplug, so the wording covers both), before `Begin` goes out. The
/// question is persisted: another restart before it's answered asks it again, and only "yes"
/// sends `Begin`.
#[test]
fn an_app_restart_after_n_asks_the_same_question() {
    for (i, (point, reached)) in after_n_points().into_iter().enumerate() {
        let mut w = World::new(2, 2, 161 + i as u64);
        w.pump_until(reached);
        let n = w.phone.n.unwrap();
        w.interruption_answer = None;
        let frames = w.frames.len();
        w.cut_app(true);
        w.pump();
        assert!(w.run.coordinator.card_interruption_pending(w.id), "{point}");
        assert!(w
            .phone
            .has(|m| matches!(m, CoordCardMutation::CheckInterrupted { .. })));
        assert_eq!(
            w.frames.len(),
            frames,
            "{point}: no Begin before the answer"
        );
        // Closed again, unanswered: asked again.
        w.cut_app(true);
        w.pump();
        assert!(w.run.coordinator.card_interruption_pending(w.id), "{point}");
        assert_eq!(w.frames.len(), frames);
        w.phone_act(PhoneAction::AnswerInterruption(true));
        w.pump();
        w.assert_passed(point);
        assert_eq!(w.phone.n, Some(n));
    }
}

/// The app's leave (the page's Close): a `Cancel` sends the device home, and the flow's
/// session is forgotten; the app persists what core staged.
fn leave_the_page(w: &mut World) {
    w.run.coordinator.card_flow_left(w.id);
    w.flush_phone();
    let flash = w.run.start_devices[&w.id].clone();
    w.run.devices.insert(w.id, flash);
    w.session = None;
    w.screen = None;
    w.wire.clear();
    w.resend = None;
    w.phone_actions.clear();
    w.covered_actions.clear();
}

/// Review F1, owner B9 and B12: leaving the page while the D check runs is an interruption too,
/// persisted, so the flow opened again asks whether the user caused it before anything goes to
/// the device. "Yes" resumes on the same N and passes; "no" marks the device suspect. Before N
/// it asks nothing.
#[test]
fn leaving_the_page_after_n_asks_the_question() {
    for answer in [true, false] {
        let mut w = World::new(2, 2, 171);
        w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { k: 2, .. })));
        let n = w.phone.n.expect("N is out");
        leave_the_page(&mut w);
        assert!(
            w.phone
                .has(|m| matches!(m, CoordCardMutation::CheckInterrupted { .. })),
            "persisted"
        );
        let frames = w.frames.len();
        w.interruption_answer = None;
        w.phone_actions.push_back(PhoneAction::Begin);
        w.pump();
        assert!(w.run.coordinator.card_interruption_pending(w.id));
        assert!(
            !w.frames[frames..]
                .iter()
                .any(|f| matches!(f, Frame::ToDevice(_))),
            "nothing to the device before the answer"
        );
        w.interruption_answer = Some(answer);
        w.phone_act(PhoneAction::AnswerInterruption(answer));
        w.pump();
        if answer {
            w.assert_passed("left, reopened, yes");
            assert_eq!(w.phone.n, Some(n));
        } else {
            w.assert_aborted(CardFailReason::Interrupted);
        }
    }
    // Before N: nothing to ask.
    let mut w = World::new(2, 2, 172);
    w.auto_confirm = false;
    w.pump();
    leave_the_page(&mut w);
    assert!(!w
        .phone
        .has(|m| matches!(m, CoordCardMutation::CheckInterrupted { .. })));
    w.auto_confirm = true;
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump();
    w.assert_passed("left before N");
}

/// The chip freezes its device round: it shows its round and never sends `DeviceRound`.
fn chip_freezes_its_round(screen: &CardScreen) -> bool {
    matches!(
        screen,
        CardScreen::DeviceRoundIntro | CardScreen::DeviceRoundAnswer { .. }
    )
}

/// Review F1: after N a chip that saw D hit its lie freezes its device round, so the phone owes
/// it no deadline there. The user's way out, the page's Close, used to ask nothing, so the chip
/// was never marked. Now it asks, and the user who says the device froze ("no") marks it
/// suspect.
#[test]
fn a_frozen_device_round_left_by_close_is_attributed() {
    let mut w = World::new(2, 2, 91);
    let truth = w.truth;
    w.device_lies = (0..6u8)
        .map(|j| (j, (truth[j as usize] + 1) % 2048))
        .collect();
    w.device_holds = Some(chip_freezes_its_round);
    w.pump();
    assert!(w.phone.d.is_some(), "the chip has seen D");
    assert!(matches!(w.events.last(), Some(CardEvent::DeviceRound)));
    leave_the_page(&mut w);
    w.device_holds = None;
    let events = w.events.len();
    w.interruption_answer = Some(false);
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump();
    assert!(
        w.events[events..].iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                reason: CardFailReason::Interrupted,
                device_suspect: true,
                ..
            }
        )),
        "{:?}",
        &w.events[events..]
    );
    assert!(w.suspect());
    assert_eq!(w.status(), CardBackupStatus::Failed);
}

/// Owner B9: after "yes", a device that reports anything but this check on the same N is
/// marked suspect (the "set up again without this device" alarm): one rolled back to before
/// N (after it revealed its coin), to before the writing, or to another N.
#[test]
fn yes_then_a_device_in_another_state_is_suspect() {
    let mut w = World::new(2, 2, 181);
    w.auto_confirm = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::CardChecked));
    let before_n = w.run.start_devices[&w.id].clone();
    let sealed = {
        let mut w = World::new(2, 2, 181);
        w.filter = Some(Box::new(|frame| match frame {
            Frame::ToDevice(CoordinatorCard::WriteStart { .. }) => Tamper::Drop,
            frame => Tamper::Pass(frame),
        }));
        w.pump();
        assert_eq!(w.state(), Some(CardState::Sealed));
        w.run.start_devices[&w.id].clone()
    };
    let rollbacks = [("before N", before_n), ("before the writing", sealed)];
    for (what, flash) in rollbacks {
        let mut w = World::new(2, 2, 181);
        w.pump_until(|w| matches!(w.screen, Some(CardScreen::DeviceRoundAnswer { .. })));
        assert!(w.phone.d.is_some(), "the coin is recorded");
        w.replace_device(flash, vec![]);
        w.pump();
        assert_eq!(w.status(), CardBackupStatus::Failed, "{what}");
        assert!(w.suspect(), "{what}");
        assert!(w
            .events
            .iter()
            .any(|e| matches!(e, CardEvent::Interrupted { .. })));
    }

    // Another N: the device took a second phone's N (it rewrote its record).
    let mut w = World::new(2, 2, 182);
    let flash = w.run.start_devices[&w.id].clone();
    w.pump_until(|w| {
        w.record().is_some_and(|rec| rec.n_tag.is_some())
            && w.events
                .iter()
                .any(|e| matches!(e, CardEvent::PhonePage { page: 1, .. }))
    });
    let mut log = w.device_log.clone();
    let proving = log
        .iter()
        .position(|m| matches!(m, DeviceCardMutation::PhoneRound { .. }))
        .unwrap();
    let DeviceCardMutation::PhoneRound { share_image, d, .. } = log[proving].clone() else {
        unreachable!()
    };
    log[proving] = DeviceCardMutation::PhoneRound {
        share_image,
        n_tag: [9; 32],
        d,
    };
    log.truncate(proving + 1);
    w.replace_device(flash, log);
    w.pump();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
}

/// Owner B9: a device that drops its session while the D check runs, with no unplug the app
/// saw, is an interruption too: the question, not a silent `Begin`.
#[test]
fn a_device_that_loses_its_session_after_n_is_an_interruption() {
    let mut w = World::new(2, 2, 191);
    w.pump_until(|w| matches!(w.screen, Some(CardScreen::Gate { .. })));
    chip_takes_over(&mut w);
    w.interruption_answer = None;
    let sent = chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        },
    )
    .unwrap();
    assert!(sent.is_empty(), "{sent:?}");
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::Interrupted { .. })));
    assert!(!w.events.iter().any(|e| matches!(e, CardEvent::NeedsBegin)));
    assert!(w.run.coordinator.card_interruption_pending(w.id));
}

/// Owner B9: the device's owed work while the D check runs, which the protocol puts a deadline
/// on: its `Status`, the coin, the proofs, then each opening; nothing while the user writes or
/// answers. A device that withholds it is aborted (`card_device_timeout`), suspect; the phone
/// refuses the timeout when nothing is owed.
#[test]
fn the_device_owes_work_after_n() {
    let mut w = World::new(2, 2, 201);
    w.auto_confirm = false;
    w.pump();
    assert_eq!(w.run.coordinator.card_device_work(w.id), None, "before N");
    assert!(w.run.coordinator.card_device_timeout(w.id).is_err());

    // The chip withholds its bit proofs.
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::BitProofs { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.phone_act(PhoneAction::ConfirmDeviceHalf);
    w.pump_until(|w| w.phone.d.is_some());
    assert_eq!(
        w.run.coordinator.card_device_work(w.id),
        Some((CardDeviceWork::Proofs, 0))
    );
    let sends = w.run.coordinator.card_device_timeout(w.id).unwrap();
    w.phone_out(sends);
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert!(w.events.iter().any(|e| matches!(
        e,
        CardEvent::Failed {
            reason: CardFailReason::PhoneTimeout,
            device_suspect: true,
            ..
        }
    )));
    assert_eq!(w.run.coordinator.card_device_work(w.id), None);

    // An honest run: the work goes Coin, Proofs, Opening, and is none while the user writes
    // once every opening is in, and in the device round and phone round's questions.
    let mut w = World::new(2, 2, 202);
    let mut seen = vec![];
    for _ in 0..200_000 {
        if let Some((work, _)) = w.run.coordinator.card_device_work(w.id) {
            if seen.last() != Some(&work) {
                seen.push(work);
            }
        }
        let Some(frame) = w.wire.pop_front() else {
            if let Some(action) = w.phone_actions.pop_front() {
                w.phone_act(action);
                continue;
            }
            if !w.user_act() && !w.step_job() {
                match w.resend.take() {
                    Some(m) => w.wire.push_back(Frame::ToDevice(m)),
                    None => break,
                }
            }
            continue;
        };
        w.deliver(frame);
    }
    w.assert_passed("honest");
    assert_eq!(seen.first(), Some(&CardDeviceWork::Coin), "{seen:?}");
    assert!(seen.contains(&CardDeviceWork::Proofs), "{seen:?}");
    assert!(
        seen.contains(&CardDeviceWork::Opening {
            proofs_first: false
        }),
        "{seen:?}"
    );

    // A resume in PROVING with every proof already verified: the device computes its proofs
    // again before the next opening, and the work says so.
    let mut w = World::new(2, 2, 203);
    w.pump_until(|w| {
        w.run.coordinator.card_device_work(w.id)
            == Some((
                CardDeviceWork::Opening {
                    proofs_first: false,
                },
                0,
            ))
    });
    w.cut_device(0);
    w.pump_until(|w| {
        matches!(
            w.run.coordinator.card_device_work(w.id),
            Some((CardDeviceWork::Opening { .. }, _))
        )
    });
    assert!(matches!(
        w.run.coordinator.card_device_work(w.id),
        Some((CardDeviceWork::Opening { proofs_first: true }, _))
    ));
}

/// Audit-lying-chip F2, from the PoC `audit_f2_second_phone_rerolls_d`, fixed by owner B9. A
/// chip with a 6-word lie takes phone A's N (so learns D_A), then stalls by dropping off the
/// link, rolls its flash back to before N, and is plugged into phone B, which has no record of
/// the card. Phone B didn't start this backup, so it never draws N_B: no fresh D, no check, and
/// never Checked. Phone A, back with the device, asks about the disconnect: "yes" meets the
/// rolled-back device and marks it suspect; "no" marks it suspect at once.
#[test]
fn audit_f2_second_phone_cant_reach_checked() {
    for answer in [true, false] {
        let mut w = World::new(2, 2, 91);
        let truth = w.truth;
        w.device_lies = (0..6u8)
            .map(|j| (j, (truth[j as usize] + 1) % 2048))
            .collect();
        let mut b = Phone::new(w.run.coordinator.clone());
        // Phone A, up to its confirm of words 1–32: the chip's flash before any N.
        w.auto_confirm = false;
        w.pump();
        assert_eq!(w.state(), Some(CardState::CardChecked));
        let pre_n_flash = w.run.start_devices[&w.id].clone();
        w.auto_confirm = true;
        w.phone_actions.push_back(PhoneAction::ConfirmDeviceHalf);
        w.pump_until(|w| w.phone.d.is_some());
        let d_a = w.phone.d.unwrap();
        assert!(w.record().unwrap().n_tag.is_some(), "the chip has N_A");

        // The chip drops off the link, and the user doesn't answer on phone A yet.
        w.interruption_answer = None;
        w.phone_actions.clear();
        w.cut_device(0);
        assert!(w.run.coordinator.card_interruption_pending(w.id));
        assert_eq!(w.status(), CardBackupStatus::NotChecked);

        // The chip rolls its flash back to before N, and the user moves to phone B.
        w.swap_phone(&mut b);
        w.interruption_answer = Some(true);
        let frames = w.frames.len();
        w.events.clear();
        w.replace_device(pre_n_flash.clone(), vec![]);
        w.pump();
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, CardEvent::StartedOnAnotherPhone)),
            "{:?}",
            w.events
        );
        assert!(w.phone.log.is_empty(), "phone B records nothing");
        assert_eq!(w.phone.n, None, "phone B never draws N");
        assert_eq!(w.phone.d, None, "so the chip gets no fresh D");
        assert!(w.frames[frames..]
            .iter()
            .all(|f| !matches!(f, Frame::ToDevice(m) if !matches!(m, CoordinatorCard::Begin(_)))));
        assert_ne!(
            w.status(),
            CardBackupStatus::Checked,
            "phone B: never Checked"
        );
        assert_eq!(w.state(), Some(CardState::CardChecked));

        // Back to phone A, with the rolled-back device.
        w.swap_phone(&mut b);
        w.interruption_answer = Some(answer);
        w.events.clear();
        w.phone_act(PhoneAction::AnswerInterruption(answer));
        w.phone_actions.push_back(PhoneAction::Begin);
        w.pump();
        assert_eq!(w.status(), CardBackupStatus::Failed, "answer {answer}");
        assert!(w.suspect(), "answer {answer}");
        assert_eq!(w.phone.d, Some(d_a), "D was never drawn again");
        let wrong = (0..HALF).filter(|&j| w.card[j] != Some(truth[j])).count();
        assert_eq!(wrong, 6);
        let target = codec::Target::Share {
            image: w.share_image,
            threshold: 2,
        };
        assert!(codec::decode(&w.card, Some(&target), None).is_err());
    }
}

// ---------------- pre-PR review fixes (2 Nov) ----------------

/// The runs the device spent on each box of D, in D's order.
fn d_runs(w: &mut World) -> Vec<u8> {
    let rec = w.record().unwrap();
    rec.d
        .unwrap()
        .iter()
        .map(|&j| rec.runs[j as usize])
        .collect()
}

/// Review P1: an app restart after N loses the D words the phone verified in RAM. Fetching them
/// all again at once spent a run on every box per restart, so the 4th honest restart met the run
/// cap and marked the device suspect. After a restart an opening is fetched only in the phone
/// round, for the box it asks: restarts while words 33–64 are written, or before [Continue],
/// spend nothing, and the card passes. Each restart asks B9's question, and owner R2-2 allows 3
/// resumes per check, so the run is 2 restarts before [Continue] and 1 after.
#[test]
fn app_restarts_after_n_dont_spend_runs() {
    let mut w = World::new(2, 2, 901);
    w.auto_final_check = false;
    w.pump();
    assert_eq!(
        d_runs(&mut w),
        vec![1; 5],
        "B8: every opening fetched once, ahead"
    );
    for restart in 1..MAX_INTERRUPTION_RESUMES {
        w.cut_app(true);
        w.pump();
        assert_ne!(w.status(), CardBackupStatus::Failed, "restart {restart}");
        assert!(!w.suspect(), "restart {restart}");
        assert_eq!(
            d_runs(&mut w),
            vec![1; 5],
            "restart {restart}: no run spent"
        );
    }
    w.auto_final_check = true;
    w.cut_app(true);
    w.pump();
    w.assert_passed("after three restarts");
    // The phone round fetched each box once more: one run per box for the restart.
    assert!(
        d_runs(&mut w).iter().all(|&runs| runs <= 2),
        "{:?}",
        d_runs(&mut w)
    );
}

/// Review P2: after "yes", the device is plugged into a daisy chain rather than straight into
/// the phone, and refuses `Begin` with `NotDirectUsb`. That's a link that can't run the flow,
/// not a stall: no deadline runs on the owed `Status`, so the honest device isn't timed out and
/// marked suspect while the user re-plugs it. Plugged in straight, the check resumes.
#[test]
fn not_direct_usb_after_n_owes_no_deadline() {
    let mut w = World::new(2, 2, 911);
    w.pump_until(after_n_points()[0].1);
    w.interruption_answer = None;
    w.cut_device(0);
    w.pump();
    assert!(w.run.coordinator.card_interruption_pending(w.id));
    let sends = w
        .run
        .coordinator
        .card_interruption_answer(w.id, true)
        .unwrap();
    w.phone_out(sends);
    w.phone_actions.clear();
    w.covered_actions.clear();
    assert_eq!(
        w.run.coordinator.card_device_work(w.id),
        Some((CardDeviceWork::Status, 0))
    );
    let events = w.events.len();
    // Re-attack RA-1: replugged behind another device, which the app sees on its port.
    w.run.coordinator.card_port_shared(w.id, true);
    chip_says(
        &mut w,
        DeviceCard::Refused {
            reason: CardRefusal::NotDirectUsb,
        },
    )
    .unwrap();
    assert!(matches!(
        w.events[events..],
        [CardEvent::Refused {
            reason: CardRefusal::NotDirectUsb
        }]
    ));
    assert_eq!(w.run.coordinator.card_device_work(w.id), None);
    assert!(w.run.coordinator.card_device_timeout(w.id).is_err());
    assert!(!w.suspect());
    // Unplugged from the chain: the question again; "yes", and straight into the phone.
    w.interruption_answer = Some(true);
    w.cut_device(0);
    w.pump();
    w.assert_passed("plugged in straight");
}

/// Asserts the check ended in an abort with the device suspect (re-attack RA-1).
fn assert_not_direct_usb_abort(w: &World, what: &str) {
    assert!(w.suspect(), "{what}");
    assert_eq!(w.status(), CardBackupStatus::Failed, "{what}");
    assert_eq!(w.run.coordinator.card_device_work(w.id), None, "{what}");
    assert!(
        w.events.iter().any(|e| matches!(
            e,
            CardEvent::Failed {
                device_suspect: true,
                reason: CardFailReason::Refused(CardRefusal::NotDirectUsb),
                ..
            }
        )),
        "{what}: {:?}",
        w.events
    );
}

/// Re-attack RA-1 (1): a chip that owes proofs sends `NotDirectUsb` with no `Begin` waiting
/// for its reply. An honest device sends it only as that reply, so it's a refusal after N: an
/// abort with the device suspect, never a paused deadline. That holds even on a port the app
/// sees shared, at every point after N.
#[test]
fn an_unasked_not_direct_usb_after_n_is_an_abort() {
    for (point, until) in after_n_points() {
        for shared in [false, true] {
            let what = format!("{point}, shared port {shared}");
            let mut w = World::new(2, 2, 7001);
            w.pump_until(until);
            assert!(w.run.coordinator.card_device_work(w.id).is_some(), "{what}");
            chip_takes_over(&mut w);
            w.run.coordinator.card_port_shared(w.id, shared);
            w.events.clear();
            chip_says(
                &mut w,
                DeviceCard::Refused {
                    reason: CardRefusal::NotDirectUsb,
                },
            )
            .unwrap();
            assert_not_direct_usb_abort(&w, &what);
        }
    }
}

/// Re-attack RA-1 (2): after an unplug and the user's "yes", the chip answers the `Begin`
/// with `NotDirectUsb`, though the app sees it alone on its port, so plugged straight in. It's
/// the B9 refusal of the resume: an abort with the device suspect, not "plug it in straight"
/// over and over with nothing owed.
#[test]
fn a_not_direct_usb_from_a_device_alone_on_its_port_is_an_abort() {
    for (point, until) in after_n_points() {
        let mut w = World::new(2, 2, 7002);
        w.pump_until(until);
        w.interruption_answer = None;
        w.cut_device(0);
        w.pump();
        assert!(w.run.coordinator.card_interruption_pending(w.id), "{point}");
        let sends = w
            .run
            .coordinator
            .card_interruption_answer(w.id, true)
            .unwrap();
        w.phone_out(sends);
        w.phone_actions.clear();
        w.covered_actions.clear();
        assert_eq!(
            w.run.coordinator.card_device_work(w.id),
            Some((CardDeviceWork::Status, 0)),
            "{point}"
        );
        w.run.coordinator.card_port_shared(w.id, false);
        w.events.clear();
        chip_says(
            &mut w,
            DeviceCard::Refused {
                reason: CardRefusal::NotDirectUsb,
            },
        )
        .unwrap();
        assert_not_direct_usb_abort(&w, point);
    }
}

/// Review P3: a resume where the phone held only some of the bit proofs. It pulls the missing
/// ones, which the device computes first, then wraps to compute the rest before any opening. So
/// the opening owed then is `proofs_first`, with the longer budget, though the phone pulled a
/// frame in this session.
#[test]
fn a_partial_proofs_resume_owes_an_opening_after_the_proofs() {
    let mut w = World::new(2, 2, 921);
    w.pump_until(|w| w.phone.verified.len() >= 280 && w.phone.verified.len() < 311);
    w.cut_device(0);
    let mut seen = vec![];
    w.pump_until(|w| {
        let work = w.run.coordinator.card_device_work(w.id);
        if seen.last() != Some(&work) {
            seen.push(work);
        }
        matches!(work, Some((CardDeviceWork::Opening { .. }, _)))
    });
    assert!(
        seen.iter()
            .any(|work| matches!(work, Some((CardDeviceWork::Proofs, _)))),
        "the phone pulled its missing proofs: {seen:?}"
    );
    assert!(
        matches!(
            seen.last(),
            Some(Some((CardDeviceWork::Opening { proofs_first: true }, _)))
        ),
        "{seen:?}"
    );
    w.pump();
    w.assert_passed("after the partial resume");
}

/// Review P4: "No, I didn't" with the device still unplugged mid device round. The app drops
/// `PhoneRound{n, false}` (the device is out), so the device never hears the abort. A later
/// flow with the device sends that result again, never `FinalBegin`: the device round isn't
/// resumed on a failed record, and the device ends FAILED. A `DeviceRound{true}` that comes
/// anyway records nothing.
#[test]
fn no_while_unplugged_in_the_device_round_ends_on_the_device_later() {
    let mut w = World::new(2, 2, 931);
    w.pump_until(after_n_points()[1].1);
    w.interruption_answer = None;
    w.cut_device(0);
    w.pump();
    assert!(w.run.coordinator.card_interruption_pending(w.id));
    // The app drops the to-device sends: the device is out.
    let _ = w
        .run
        .coordinator
        .card_interruption_answer(w.id, false)
        .unwrap();
    w.flush_phone();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(w.suspect());
    assert_eq!(w.state(), Some(CardState::Checking));
    // A pass the device sends for the round now is stale.
    let log = w.phone.log.len();
    chip_says(&mut w, DeviceCard::DeviceRound { pass: true }).unwrap();
    assert!(
        !w.phone.log[log..]
            .iter()
            .any(|m| matches!(m, CoordCardMutation::DeviceRoundResult { .. })),
        "{:?}",
        &w.phone.log[log..]
    );
    let (events, frames) = (w.events.len(), w.frames.len());
    w.phone_actions.push_back(PhoneAction::Begin);
    w.pump();
    let to_device: Vec<_> = w.frames[frames..]
        .iter()
        .filter_map(|f| match f {
            Frame::ToDevice(m) => Some(m.kind()),
            _ => None,
        })
        .collect();
    assert_eq!(to_device, vec!["Begin", "PhoneRound"], "no FinalBegin");
    assert!(
        !w.events[events..]
            .iter()
            .any(|e| matches!(e, CardEvent::DeviceRound | CardEvent::PreparingBox { .. })),
        "{:?}",
        &w.events[events..]
    );
    assert!(matches!(
        w.events.last(),
        Some(CardEvent::Failed {
            reason: CardFailReason::Interrupted,
            device_suspect: true,
            ..
        })
    ));
    assert_eq!(
        w.state(),
        Some(CardState::Failed(VerdictReason::PhoneRoundFailed))
    );
    assert_eq!(w.status(), CardBackupStatus::Failed);
}

/// Review F3: after N, two device reports no honest device can give are aborts, the device
/// suspect: CHECKING with this phone's n_tag while this phone never started the device round
/// (so never sent `FinalBegin`), and a status in another card format.
#[test]
fn impossible_reports_after_n_are_aborts() {
    for report in ["checking-with-our-n", "other-format"] {
        let mut w = World::new(2, 2, 211);
        w.pump_until(after_n_points()[0].1);
        let n = w.phone.n.expect("N is out");
        let mut status = chip_takes_over(&mut w);
        // The chip drops its session; the user says "yes".
        w.interruption_answer = None;
        chip_says(
            &mut w,
            DeviceCard::Refused {
                reason: CardRefusal::NoSession,
            },
        )
        .unwrap();
        assert!(w.run.coordinator.card_interruption_pending(w.id));
        w.phone_act(PhoneAction::AnswerInterruption(true));
        while let Some(action) = w.phone_actions.pop_front() {
            w.phone_act(action);
        }
        w.wire.clear();
        status.check = CardCheckReport::Started {
            n_tag: proof::n_tag(&n, &mut Ops::default()),
            runs: 0,
        };
        match report {
            "checking-with-our-n" => status.state = CardStateReport::Checking,
            _ => {
                status.state = CardStateReport::Proving;
                status.format_id ^= 0x40;
            }
        }
        w.events.clear();
        let sent = chip_says(&mut w, DeviceCard::Status(Box::new(status))).unwrap();
        assert!(w.suspect(), "{report}");
        assert_eq!(w.status(), CardBackupStatus::Failed, "{report}");
        assert!(
            w.events.iter().any(|e| matches!(
                e,
                CardEvent::Failed {
                    device_suspect: true,
                    ..
                }
            )),
            "{report}: {:?}",
            w.events
        );
        assert!(
            sent.iter()
                .any(|m| matches!(m, CoordinatorCard::PhoneRound { pass: false, .. })),
            "{report}: {sent:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|m| matches!(m, CoordinatorCard::OpenStart { .. })),
            "{report}"
        );
    }
}

/// Review T2, owner B9: a chip that passes its device round and then never shows its gate
/// (its `GateShown` never comes) owes the gate, and the deadline on it is an abort with the
/// device suspect: the phone round's questions wait for that gate.
#[test]
fn a_device_that_never_shows_its_gate_owes_it() {
    let mut w = World::new(2, 2, 204);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToPhone(DeviceCard::GateShown) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert!(w.phone.round_passed, "the device round passed");
    assert!(!w
        .events
        .iter()
        .any(|e| matches!(e, CardEvent::Question { .. })));
    assert_eq!(
        w.run.coordinator.card_device_work(w.id),
        Some((CardDeviceWork::Gate, 0))
    );
    let sends = w.run.coordinator.card_device_timeout(w.id).unwrap();
    w.phone_out(sends);
    w.pump();
    w.assert_aborted(CardFailReason::PhoneTimeout);
}

/// Review T3, owner B10: a card that already failed with no suspect mark (a Stop before N,
/// in a pre-release build's record, whose verdict the device never got) and then a device showing
/// another seal: the device is marked suspect there, and the alarm says so.
#[test]
fn another_seal_on_a_failed_record_marks_the_device() {
    let mut w = World::new(2, 2, 205);
    w.auto_confirm = false;
    w.pump();
    assert_eq!(w.state(), Some(CardState::CardChecked));
    // The record an earlier build's Stop before N left, with no verdict from the device.
    w.run
        .coordinator
        .mutate(CoordMutation::Card(CoordCardMutation::Failed {
            access_structure_ref: w.asref,
            share_index: w.share_index,
            reason: CardFailReason::Stopped,
        }));
    w.flush_phone();
    assert_eq!(w.status(), CardBackupStatus::Failed);
    assert!(!w.suspect());
    let rec = w
        .run
        .coordinator
        .card_record(w.asref, w.share_index)
        .unwrap()
        .clone();
    assert_eq!(rec.device_passed, None);
    // The device comes back showing another seal.
    w.unplugged();
    w.phone_actions.clear();
    let mut status = chip_takes_over(&mut w);
    status.state = CardStateReport::CardChecked;
    status.check = CardCheckReport::None;
    status.sealed.h_c[0] ^= 1;
    w.phone_act(PhoneAction::Begin);
    w.wire.clear();
    w.events.clear();
    chip_says(&mut w, DeviceCard::Status(Box::new(status))).unwrap();
    assert!(w.suspect());
    assert!(
        matches!(
            w.events[..],
            [CardEvent::Failed {
                reason: CardFailReason::SealChanged,
                device_suspect: true,
                ..
            }]
        ),
        "{:?}",
        w.events
    );
}

/// A genuine SEALED flash of `World::new(2, 2, seed)`'s device: what a chip reports, whatever it
/// went through since.
fn sealed_flash(seed: u64) -> FrostSigner {
    let mut w = World::new(2, 2, seed);
    w.filter = Some(Box::new(|frame| match frame {
        Frame::ToDevice(CoordinatorCard::WriteStart { .. }) => Tamper::Drop,
        frame => Tamper::Pass(frame),
    }));
    w.pump();
    assert_eq!(w.state(), Some(CardState::Sealed));
    w.run.start_devices[&w.id].clone()
}

/// Second review, lying-chip F2 (PoC `audit2_f2_frozen_round_then_sealed_rerolls_d_on_another_phone`),
/// closed by owner B15. A chip with a 6-word lie sees D on the keygen install's phone, D hits
/// its lie, and it freezes its device round; the user leaves. It then tells another install with
/// the wallet that its card is fresh (SEALED, under the same deterministic seal), to have that
/// phone draw its own N and give it a second D. That install didn't run the keygen, so it never
/// pins, draws N, pulls proofs or openings: it writes the card unverified, and its record is
/// "written, not verified", never Checked. The chip only ever saw one D, and the card, which
/// doesn't restore, is never shown as checked anywhere. The keygen install still asks about the
/// interruption, and "No, I didn't" marks the chip.
#[test]
fn a_chip_that_says_sealed_to_another_install_gets_no_fresh_d() {
    let seed = 91;
    let sealed = sealed_flash(seed);
    let d_a = {
        let mut w = World::new(2, 2, seed);
        w.device_holds = Some(chip_freezes_its_round);
        w.pump();
        w.phone.d.expect("the chip has seen D")
    };
    let mut w = World::new(2, 2, seed);
    let truth = w.truth;
    let mut lies = vec![d_a[0]];
    lies.extend((0..HALF as u8).filter(|j| !d_a.contains(j)).take(5));
    w.device_lies = lies
        .iter()
        .map(|&j| (j, (truth[j as usize] + 1) % 2048))
        .collect();
    w.device_holds = Some(chip_freezes_its_round);
    w.pump();
    assert_eq!(w.phone.d, Some(d_a), "the same D, and it hits the lie");
    assert!(matches!(w.events.last(), Some(CardEvent::DeviceRound)));
    // The user gives up on the keygen install's phone: leaving the page is an interruption
    // (review F1), asked about when the flow opens again.
    w.run.coordinator.card_flow_left(w.id);
    w.phone_out(vec![]);
    assert!(w.run.coordinator.card_interruption_pending(w.id));
    assert_eq!(w.status(), CardBackupStatus::NotChecked);

    // Another install with the wallet. The chip just reports SEALED.
    let mut keygen_phone = other_install(2, 2, seed);
    w.swap_phone(&mut keygen_phone);
    assert!(!w.created_here());
    w.device_holds = None;
    w.phone_actions.clear();
    let frames = w.frames.len();
    w.replace_device(sealed, vec![]);
    w.pump();
    assert!(
        matches!(w.events.last(), Some(CardEvent::UnverifiedWritten)),
        "{:?}",
        w.events.last()
    );
    assert!(w.errors.is_empty(), "{:?}", w.errors);
    // No N, no fresh D: nothing of the verified check left this phone or was recorded.
    assert!(!w.phone.has(|m| matches!(
        m,
        CoordCardMutation::Pinned { .. }
            | CoordCardMutation::CheckStarted { .. }
            | CoordCardMutation::BoxesDrawn { .. }
    )));
    assert!(w.frames[frames..].iter().all(|f| !matches!(
        f,
        Frame::ToDevice(
            CoordinatorCard::PullCommit { .. }
                | CoordinatorCard::PullProofs { .. }
                | CoordinatorCard::DeviceHalfWritten { .. }
                | CoordinatorCard::OpenStart { .. }
                | CoordinatorCard::OpenBox { .. }
                | CoordinatorCard::FinalBegin
        )
    )));
    let rec = w.record().unwrap();
    assert_eq!(rec.n_tag, None, "the chip got no second N");
    assert_eq!(rec.d, None, "and no second D");
    assert_eq!(w.status(), CardBackupStatus::Unverified, "never Checked");
    let wrong = (0..HALF).filter(|&j| w.card[j] != Some(truth[j])).count();
    assert_eq!(wrong, 6);
    let target = codec::Target::Share {
        image: w.share_image,
        threshold: 2,
    };
    assert!(codec::decode(&w.card, Some(&target), None).is_err());

    // Back on the keygen install's phone: the question, and "No, I didn't" marks the chip.
    w.swap_phone(&mut keygen_phone);
    assert!(w.run.coordinator.card_interruption_pending(w.id));
    w.phone_act(PhoneAction::AnswerInterruption(false));
    assert!(w.suspect());
    assert_eq!(w.status(), CardBackupStatus::Failed);
}
