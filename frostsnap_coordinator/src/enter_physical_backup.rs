use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{
        card::{CardRestoreEvent, CardRestoreToUser},
        restoration, CoordinatorToUserMessage,
    },
    message::{
        card::{CardHalfEntries, RestoreResult},
        CoordinatorRestoration, CoordinatorToDeviceMessage,
    },
    schnorr_fun::frost::SharedKey,
    DeviceId, EnterPhysicalId,
};
use std::sync::{Arc, Mutex};

use crate::{card_backup::refusal_message, DeviceMode, Sink, UiProtocol};

/// Words in each half of a card.
const HALF: usize = 32;

pub struct EnterPhysicalBackup {
    sink: Box<dyn Sink<EnterPhysicalBackupState>>,
    enter_physical_id: EnterPhysicalId,
    chosen_device: DeviceId,
    sent_req: bool,
    connected: bool,
    entered: Option<restoration::PhysicalBackupPhase>,
    saved: bool,
    abort: Option<String>,
    /// A 64-word card instead of a 25-word backup: what goes in its `EnterCard`.
    card: Option<CardRestoreRequest>,
    /// A card restore's words 33–64, which the app's entry sets.
    phone_half: CardPhoneHalfHandle,
    /// The device's latest `RestoreState` for this restore's `EnterCard`: none until it
    /// acknowledged it.
    device_state: Option<(bool, bool, bool)>,
    /// The generation of the phone half last sent, and whether a `RestoreState` came since.
    sent_generation: Option<u32>,
    state_since_send: bool,
    /// The last decode's result; the view follows it until the next decode starts.
    decoded: Option<RestoreResult>,
    /// Another protocol's `Cancel` already went to every device.
    devices_cancelled: bool,
}

/// The app's words 33–64 for one card restore, with a generation that every [`set`](Self::set)
/// bumps. The protocol sends them to the device only once it acknowledged the restore, and
/// again after an edit or whenever the device says it lacks them (design §3.7). The words are
/// wiped when the last handle is dropped.
#[derive(Clone, Default)]
pub struct CardPhoneHalfHandle(Arc<Mutex<Option<(u32, CardHalfEntries)>>>);

impl CardPhoneHalfHandle {
    /// The user's words 33–64: a word index each, or `None` for "Can't read this word".
    pub fn set(&self, words: [Option<u16>; HALF]) {
        let mut slot = self.0.lock().unwrap();
        let generation = slot.as_ref().map_or(1, |(generation, _)| generation + 1);
        *slot = Some((generation, CardHalfEntries(words)));
    }

    fn get(&self) -> Option<(u32, CardHalfEntries)> {
        self.0.lock().unwrap().clone()
    }
}

impl core::fmt::Debug for CardPhoneHalfHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CardPhoneHalfHandle(<redacted>)")
    }
}

impl EnterPhysicalBackup {
    pub fn new(sink: impl Sink<EnterPhysicalBackupState>, chosen_device: DeviceId) -> Self {
        let enter_physical_id = EnterPhysicalId::new(&mut rand::thread_rng());
        Self {
            sink: Box::new(sink),
            enter_physical_id,
            chosen_device,
            sent_req: false,
            connected: false,
            entered: None,
            saved: false,
            abort: None,
            card: None,
            phone_half: CardPhoneHalfHandle::default(),
            device_state: None,
            sent_generation: None,
            state_since_send: false,
            decoded: None,
            devices_cancelled: false,
        }
    }

    /// Enter a 64-word card: words 1–32 on the device and words 33–64 in the app, which the app
    /// gives the returned handle. The device decodes once both are in, and then replies as it
    /// does for a 25-word backup. `target` is F when the app knows the wallet,
    /// `recorded_format` the card format its record of the key gives, and `share_indices` the
    /// wallet's share indices, which keep a repair within its time (build plan A6).
    pub fn new_card(
        sink: impl Sink<EnterPhysicalBackupState>,
        chosen_device: DeviceId,
        target: Option<SharedKey>,
        recorded_format: Option<u8>,
        share_indices: Option<Vec<u8>>,
    ) -> (Self, CardPhoneHalfHandle) {
        let protocol = Self {
            card: Some(CardRestoreRequest {
                target,
                recorded_format,
                share_indices,
            }),
            ..Self::new(sink, chosen_device)
        };
        let handle = protocol.phone_half.clone();
        (protocol, handle)
    }

    /// The device, for a card restore.
    pub fn card_device(&self) -> Option<DeviceId> {
        self.card.as_ref().map(|_| self.chosen_device)
    }

    /// What the phone shows of a card restore, from the device's latest `RestoreState`.
    fn card_restore_view(&self) -> Option<CardRestoreView> {
        self.card.as_ref()?;
        Some(match self.device_state {
            None => CardRestoreView::WaitingForDevice,
            Some((_, _, true)) => CardRestoreView::Checking,
            _ if self.decoded == Some(RestoreResult::CouldNotRead) => CardRestoreView::CouldNotRead,
            _ if self.decoded == Some(RestoreResult::WrongWallet) => CardRestoreView::WrongWallet,
            Some(_) if self.decoded == Some(RestoreResult::Restored) => {
                CardRestoreView::FinishOnDevice
            }
            Some((device_half_in, false, _)) => CardRestoreView::EnterHalves { device_half_in },
            Some((false, true, _)) => CardRestoreView::WaitingForDeviceHalf,
            // Both halves are in and no decode runs: the report is up on the device.
            Some((true, true, false)) => CardRestoreView::Checking,
        })
    }

    pub fn emit_state(&self) {
        self.sink.send(EnterPhysicalBackupState {
            device_id: self.chosen_device,
            entered: self.entered,
            saved: self.saved,
            abort: self.abort.clone(),
            card_restore: self.card_restore_view(),
        })
    }

    /// Ends the restore with `why`, unless it already ended.
    fn abort_with(&mut self, why: &str) {
        if self.abort.is_none() && !self.saved {
            self.abort = Some(why.into());
            self.emit_state();
        }
    }

    fn on_card_restore(&mut self, event: CardRestoreEvent) {
        match event {
            CardRestoreEvent::State {
                device_half_in,
                phone_half_in,
                decoding,
            } => {
                if decoding {
                    self.decoded = None;
                }
                self.device_state = Some((device_half_in, phone_half_in, decoding));
                self.state_since_send = true;
            }
            CardRestoreEvent::Outcome { result } => {
                self.decoded = Some(result);
                // These end the restore on the device, which then drops any half it's sent.
                match result {
                    // The masks passed, so the card read: the app clears its failure count
                    // on this exact text (`alreadyHeldAbort`, enter_card_view.dart).
                    RestoreResult::AlreadyHeld => {
                        return self.abort_with(
                            "This device already holds this key. Restore it on a blank device.",
                        )
                    }
                    RestoreResult::NeedsNewerFirmware => return self.abort_with(
                        "This card needs newer firmware. Update your device, then restore again.",
                    ),
                    // Owner B11: the device's page for another wallet's card exits the restore
                    // there, and the app shows its own page, which the user closes. Not an
                    // abort, so the device's page isn't cancelled away before it's read.
                    RestoreResult::Restored
                    | RestoreResult::CouldNotRead
                    | RestoreResult::WrongWallet => {}
                }
            }
        }
        self.emit_state();
    }

    /// The phone half, when it's due: after the device's acknowledgement, and either newer than
    /// what was sent or missing on the device by its latest state.
    fn phone_half_due(&mut self) -> Option<CoordinatorSendMessage> {
        let (_, phone_half_in, _) = self.device_state?;
        let (generation, words) = self.phone_half.get()?;
        let newer = self.sent_generation != Some(generation);
        let lost = !phone_half_in && self.state_since_send;
        if !newer && !lost {
            return None;
        }
        self.sent_generation = Some(generation);
        self.state_since_send = false;
        Some(CoordinatorSendMessage::to(
            self.chosen_device,
            CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::CardPhoneHalf {
                    enter_physical_id: self.enter_physical_id,
                    words,
                },
            )),
        ))
    }
}

impl UiProtocol for EnterPhysicalBackup {
    fn cancel(&mut self) {
        self.abort = Some("entering backup canceled".into());
    }

    fn is_complete(&self) -> Option<crate::Completion> {
        if self.saved {
            Some(crate::Completion::Success)
        } else if self.abort.is_some() {
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: !self.devices_cancelled,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if id == self.chosen_device {
            self.connected = false;
            self.abort = Some("device was unplugged".into());
        }
        self.emit_state();
    }

    /// The `Cancel` drops the device's restore, whose halves are in RAM only.
    fn devices_cancelled(&mut self) {
        if self.card.is_some() {
            self.devices_cancelled = true;
            self.abort_with("Your device was reset. Start the restore again.");
        }
    }

    fn poll(&mut self) -> Vec<frostsnap_comms::CoordinatorSendMessage> {
        if !self.connected || self.abort.is_some() {
            return vec![];
        }
        if !self.sent_req {
            self.sent_req = true;
            let enter_physical_id = self.enter_physical_id;
            let request = match self.card.clone() {
                Some(CardRestoreRequest {
                    target,
                    recorded_format,
                    share_indices,
                }) => CoordinatorRestoration::EnterCard {
                    enter_physical_id,
                    target,
                    recorded_format,
                    share_indices,
                },
                None => CoordinatorRestoration::EnterPhysicalBackup { enter_physical_id },
            };
            return vec![CoordinatorSendMessage::to(
                self.chosen_device,
                CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(request)),
            )];
        }
        if self.card.is_some() && self.entered.is_none() {
            return self.phone_half_due().into_iter().collect();
        }
        vec![]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn connected(&mut self, id: DeviceId, _state: DeviceMode) {
        if id == self.chosen_device {
            self.connected = true;
        }
        self.emit_state();
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        match message {
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupEntered(physical_backup_phase),
            ) if physical_backup_phase.from == self.chosen_device
                && physical_backup_phase.backup.enter_physical_id == self.enter_physical_id =>
            {
                self.entered = Some(*physical_backup_phase);
                self.emit_state();
                true
            }
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupSaved { device_id, .. },
            ) if device_id == self.chosen_device => {
                self.saved = true;
                self.emit_state();
                true
            }
            CoordinatorToUserMessage::CardRestore(CardRestoreToUser { device_id, event })
                if device_id == self.chosen_device && self.card.is_some() && self.sent_req =>
            {
                self.on_card_restore(event);
                true
            }
            // Such as `EnterCard` refused off a direct USB link (addendum U9).
            CoordinatorToUserMessage::CardRefused { device_id, reason }
                if device_id == self.chosen_device && self.card.is_some() =>
            {
                self.abort_with(refusal_message(reason));
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnterPhysicalBackupState {
    pub device_id: DeviceId,
    pub entered: Option<restoration::PhysicalBackupPhase>,
    pub saved: bool,
    pub abort: Option<String>,
    /// A card restore's view; `None` for a 25-word backup.
    pub card_restore: Option<CardRestoreView>,
}

/// A card restore's `EnterCard`, as the app asks for it.
#[derive(Clone, Debug)]
struct CardRestoreRequest {
    target: Option<SharedKey>,
    recorded_format: Option<u8>,
    share_indices: Option<Vec<u8>>,
}

/// What the phone shows while a card restore runs (design §3.7, §5.3 E4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardRestoreView {
    /// `EnterCard` went out and the device hasn't acknowledged it: "Connecting to your device…"
    WaitingForDevice,
    /// The app's half isn't on the device yet. `device_half_in`: words 1–32 are in on the
    /// device.
    EnterHalves { device_half_in: bool },
    /// The app's half is on the device; words 1–32 aren't: "Sent. Now finish words 1–32 on your
    /// device."
    WaitingForDeviceHalf,
    /// Both halves are in and the device decodes them, for a fixed 10 s.
    Checking,
    /// The decode found no share, or one whose masks fail (owner B7): "Your card couldn't be
    /// read." An edit of either half decodes again, or the user tries another phone. Every
    /// failure is this same view; the app moves its line on by its own count of them (owner,
    /// 2 Nov 2026: check the words, then can't be verified, then offline recovery).
    CouldNotRead,
    /// The decode found the share and the device shows its report, threshold warnings
    /// included: "Finish on your device: hold to restore." Nothing decoded reaches the app
    /// before the hold (owner B7).
    FinishOnDevice,
    /// A genuine card for another wallet (owner B11): it decoded uniquely and its self-checking
    /// masks passed, but it isn't for the F the app sent. The device's restore ends on its own
    /// page. It's never a failed read, so the app's failure count neither counts nor clears it.
    WrongWallet,
}

#[cfg(test)]
mod test {
    use super::*;
    use frostsnap_core::message::card::CardRefusal;

    #[derive(Clone, Default)]
    struct States(Arc<Mutex<Vec<EnterPhysicalBackupState>>>);

    impl Sink<EnterPhysicalBackupState> for States {
        fn send(&self, state: EnterPhysicalBackupState) {
            self.0.lock().unwrap().push(state);
        }
    }

    impl States {
        fn last(&self) -> EnterPhysicalBackupState {
            self.0.lock().unwrap().last().cloned().expect("a state")
        }
    }

    const DEVICE: DeviceId = DeviceId([2; 33]);

    #[test]
    fn a_card_target_sends_enter_card() {
        let (mut protocol, _) =
            EnterPhysicalBackup::new_card((), DEVICE, None, Some(3), Some(vec![1, 2]));
        assert!(
            protocol.poll().is_empty(),
            "nothing before the device connects"
        );
        protocol.connected(DEVICE, DeviceMode::Blank);
        let sent = protocol.poll();
        assert!(matches!(
            sent.as_slice(),
            [CoordinatorSendMessage {
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                    CoordinatorRestoration::EnterCard {
                        target: None,
                        recorded_format: Some(3),
                        share_indices: Some(indices),
                        ..
                    }
                )),
                ..
            }] if indices == &[1, 2]
        ));
        assert!(protocol.poll().is_empty(), "sent once");
    }

    fn card_restore() -> (EnterPhysicalBackup, CardPhoneHalfHandle, States) {
        let states = States::default();
        let (mut protocol, handle) =
            EnterPhysicalBackup::new_card(states.clone(), DEVICE, None, None, None);
        protocol.connected(DEVICE, DeviceMode::Blank);
        assert_eq!(protocol.poll().len(), 1, "EnterCard");
        (protocol, handle, states)
    }

    fn state(
        device_half_in: bool,
        phone_half_in: bool,
        decoding: bool,
    ) -> CoordinatorToUserMessage {
        CoordinatorToUserMessage::CardRestore(CardRestoreToUser {
            device_id: DEVICE,
            event: CardRestoreEvent::State {
                device_half_in,
                phone_half_in,
                decoding,
            },
        })
    }

    fn outcome(result: RestoreResult) -> CoordinatorToUserMessage {
        CoordinatorToUserMessage::CardRestore(CardRestoreToUser {
            device_id: DEVICE,
            event: CardRestoreEvent::Outcome { result },
        })
    }

    fn view(states: &States) -> CardRestoreView {
        states.last().card_restore.expect("a card restore")
    }

    /// The phone's words that went out, if any.
    fn sent_half(sent: &[CoordinatorSendMessage]) -> Option<[Option<u16>; HALF]> {
        match sent {
            [] => None,
            [CoordinatorSendMessage {
                message_body:
                    CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                        CoordinatorRestoration::CardPhoneHalf { words, .. },
                    )),
                ..
            }] => Some(words.0),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Design §3.11 #14: no half before the device's acknowledgement; then the half once, again
    /// after an edit, and again when the device says it lacks it; the view follows the device.
    #[test]
    fn the_phone_half_waits_for_the_ack() {
        let (mut protocol, handle, states) = card_restore();
        let words = [Some(7); HALF];
        handle.set(words);
        assert_eq!(sent_half(&protocol.poll()), None, "no half before the ack");
        assert_eq!(view(&states), CardRestoreView::WaitingForDevice);

        assert!(protocol.process_to_user_message(state(false, false, false)));
        assert_eq!(
            view(&states),
            CardRestoreView::EnterHalves {
                device_half_in: false
            }
        );
        assert_eq!(sent_half(&protocol.poll()), Some(words));
        assert_eq!(sent_half(&protocol.poll()), None, "sent once");

        protocol.process_to_user_message(state(false, true, false));
        assert_eq!(view(&states), CardRestoreView::WaitingForDeviceHalf);
        assert_eq!(sent_half(&protocol.poll()), None);

        // An edit goes out again.
        let edited = [Some(8); HALF];
        handle.set(edited);
        assert_eq!(sent_half(&protocol.poll()), Some(edited));

        protocol.process_to_user_message(state(true, true, true));
        assert_eq!(view(&states), CardRestoreView::Checking);
        protocol.process_to_user_message(outcome(RestoreResult::CouldNotRead));
        protocol.process_to_user_message(state(true, true, false));
        assert_eq!(view(&states), CardRestoreView::CouldNotRead);
        handle.set(words);
        assert_eq!(sent_half(&protocol.poll()), Some(words));
        protocol.process_to_user_message(state(true, true, true));
        assert_eq!(view(&states), CardRestoreView::Checking);

        // The device lost it (it rebooted, say): it's sent again.
        protocol.process_to_user_message(state(false, false, false));
        assert_eq!(sent_half(&protocol.poll()), Some(words));
        assert_eq!(sent_half(&protocol.poll()), None, "once per state");
        assert!(protocol.is_complete().is_none());

        // Another device's frames aren't this restore's.
        let other = CoordinatorToUserMessage::CardRestore(CardRestoreToUser {
            device_id: DeviceId([9; 33]),
            event: CardRestoreEvent::Outcome {
                result: RestoreResult::Restored,
            },
        });
        assert!(!protocol.process_to_user_message(other));
    }

    /// A decode that ends on one of the device's exit pages ends the restore with its real
    /// reason, instead of "couldn't be read" and a page that waits for ever.
    #[test]
    fn an_exit_outcome_aborts_with_its_reason() {
        for (result, why) in [
            (
                RestoreResult::AlreadyHeld,
                "This device already holds this key. Restore it on a blank device.",
            ),
            (
                RestoreResult::NeedsNewerFirmware,
                "This card needs newer firmware. Update your device, then restore again.",
            ),
        ] {
            let (mut protocol, handle, states) = card_restore();
            handle.set([Some(7); HALF]);
            protocol.process_to_user_message(state(true, false, false));
            assert!(protocol.poll().len() == 1, "the half went out");
            protocol.process_to_user_message(state(true, true, true));
            protocol.process_to_user_message(outcome(result));
            assert_eq!(states.last().abort.as_deref(), Some(why));
            assert!(protocol.is_complete().is_some(), "{result:?}");
        }
    }

    /// The escalating restore-failure flow (owner, 2 Nov 2026) counts in the app, from what the
    /// phone saw: the protocol keeps no count and gets nothing new from the device. Each failed
    /// decode (the same frames every time, as core's `successive_failures_send_the_same_frames`
    /// checks) gives the same views, so the app's count of `CouldNotRead` turns is the only
    /// thing that moves the user on.
    #[test]
    fn successive_failures_look_alike() {
        let (mut protocol, handle, states) = card_restore();
        protocol.process_to_user_message(state(true, false, false));
        let mut runs = vec![];
        for edit in 0..3u16 {
            handle.set([Some(7 + edit); HALF]);
            assert!(sent_half(&protocol.poll()).is_some(), "each edit goes out");
            let before = states.0.lock().unwrap().len();
            protocol.process_to_user_message(state(true, true, true));
            protocol.process_to_user_message(outcome(RestoreResult::CouldNotRead));
            protocol.process_to_user_message(state(true, true, false));
            let run: Vec<_> = states.0.lock().unwrap()[before..]
                .iter()
                .map(|s| (s.card_restore, s.abort.clone(), s.saved))
                .collect();
            runs.push(run);
        }
        assert_eq!(
            runs[0],
            [
                (Some(CardRestoreView::Checking), None, false),
                // The outcome comes while the device still says it decodes.
                (Some(CardRestoreView::Checking), None, false),
                (Some(CardRestoreView::CouldNotRead), None, false),
            ]
        );
        assert!(runs.iter().all(|run| run == &runs[0]), "{runs:?}");
        assert!(protocol.is_complete().is_none(), "a failure never ends it");
    }

    /// Owner B11: another wallet's card is its own view once the decode ends, and doesn't end
    /// the protocol, so the device's own page isn't cancelled before the user reads it.
    #[test]
    fn a_wrong_wallet_outcome_is_its_own_view() {
        let (mut protocol, handle, states) = card_restore();
        handle.set([Some(7); HALF]);
        protocol.process_to_user_message(state(true, false, false));
        protocol.poll();
        protocol.process_to_user_message(state(true, true, true));
        protocol.process_to_user_message(outcome(RestoreResult::WrongWallet));
        assert_eq!(view(&states), CardRestoreView::Checking);
        protocol.process_to_user_message(state(true, true, false));
        assert_eq!(view(&states), CardRestoreView::WrongWallet);
        assert_eq!(states.last().abort, None);
        assert!(protocol.is_complete().is_none());
    }

    /// A decode that found the share points the user at the device's hold, and carries nothing
    /// decoded (owner B7).
    #[test]
    fn a_restored_outcome_points_at_the_device() {
        let (mut protocol, handle, states) = card_restore();
        handle.set([Some(7); HALF]);
        protocol.process_to_user_message(state(true, false, false));
        protocol.poll();
        protocol.process_to_user_message(state(true, true, true));
        assert_eq!(view(&states), CardRestoreView::Checking);
        protocol.process_to_user_message(outcome(RestoreResult::Restored));
        protocol.process_to_user_message(state(true, true, false));
        assert_eq!(view(&states), CardRestoreView::FinishOnDevice);
        assert!(protocol.is_complete().is_none());
        // An edit decodes again.
        protocol.process_to_user_message(state(true, true, true));
        assert_eq!(view(&states), CardRestoreView::Checking);
    }

    #[test]
    fn the_handle_debug_is_redacted() {
        let (_, handle, _) = card_restore();
        handle.set([Some(1234); HALF]);
        let debug = format!("{handle:?}");
        assert!(!debug.contains("1234"), "{debug}");
    }

    /// A refused restore, such as `EnterCard` off a direct USB link, ends the page with the
    /// reason instead of waiting for ever (addendum U9).
    #[test]
    fn a_refused_restore_aborts() {
        let (mut protocol, _, states) = card_restore();
        let refused = |device_id| CoordinatorToUserMessage::CardRefused {
            device_id,
            reason: CardRefusal::NotDirectUsb,
        };
        assert!(!protocol.process_to_user_message(refused(DeviceId([9; 33]))));
        assert!(protocol.is_complete().is_none());
        assert!(protocol.process_to_user_message(refused(DEVICE)));
        assert_eq!(
            states.last().abort.as_deref(),
            Some("Plug the device straight into this phone or computer.")
        );
        assert!(matches!(
            protocol.is_complete(),
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: true
            })
        ));
    }

    /// Another protocol's `Cancel` dropped the device's restore: the page ends, and sends no
    /// second cancel. A 25-word entry is left alone.
    #[test]
    fn a_cancel_to_every_device_ends_a_card_restore() {
        let (mut protocol, _, states) = card_restore();
        protocol.devices_cancelled();
        assert_eq!(
            states.last().abort.as_deref(),
            Some("Your device was reset. Start the restore again.")
        );
        assert!(matches!(
            protocol.is_complete(),
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: false
            })
        ));

        let mut words = EnterPhysicalBackup::new((), DEVICE);
        words.devices_cancelled();
        assert!(words.is_complete().is_none());
    }
}
