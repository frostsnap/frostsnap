use crate::keygen::SimDeviceKeyGen;
use crate::pty::Pty;
use crate::screens::{ActiveScreen, SimUiEvent};
use crate::state::{
    SharedNonceSlot, SimDeviceState, load_or_create_state, save_state_atomic, update_nonce_slots,
};
use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::*;
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use frostsnap_comms::{
    BINCODE_CONFIG, CommsMisc, CoordinatorSendBody, DeviceName, DeviceSendBody, DeviceSendMessage,
    Downstream, ReceiveSerial, Sha256Digest, Upstream, WireDeviceSendBody,
};
use frostsnap_core::device::restoration::ToUserRestoration;
use frostsnap_core::device::{DeviceToUserMessage, FrostSigner};
use frostsnap_core::device_nonces::AbSlots;
use frostsnap_core::message::signing::DeviceSigning;
use frostsnap_core::message::{DeviceSend, DeviceToCoordinatorMessage};
use frostsnap_widgets::palette::PALETTE;
use frostsnap_widgets::{Instant, SuperDrawTarget};
use rand::RngCore;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UpstreamConnectionState {
    PowerOn,
    Established,
    EstablishedAndCoordAck,
}

pub struct SimDevice {
    pub pty: Pty,
    /// Bytes waiting for the app to read. The pty buffer is small and non-blocking, so large
    /// frames go out over several loop iterations.
    pub tx_buf: VecDeque<u8>,
    pub state: SimDeviceState,
    pub state_path: PathBuf,
    pub signer: FrostSigner<SharedNonceSlot>,
    pub slot_handles: Vec<crate::state::SlotHandle>,
    pub keygen: SimDeviceKeyGen,
    pub rng: rand::rngs::ThreadRng,
    pub upstream_state: UpstreamConnectionState,
    pub recv_buf: Vec<u8>,
    pub outbox: VecDeque<DeviceSendMessage<WireDeviceSendBody>>,
    pub active_screen: ActiveScreen,
    pub headless: bool,
    pub firmware_digest: Sha256Digest,
    /// Name the app previewed during setup. Like the firmware, it is saved only once keygen
    /// finishes.
    pub pending_device_name: Option<String>,
    /// Recipients above which a send is reviewed as a summary and list. 0 always lists.
    pub list_threshold: usize,
}

impl SimDevice {
    pub fn new(
        state_path: impl AsRef<Path>,
        cli_name: Option<String>,
        headless: bool,
        firmware_digest: Sha256Digest,
        list_threshold: usize,
    ) -> io::Result<Self> {
        let state_path = state_path.as_ref().to_path_buf();
        let pty = Pty::open()?;
        println!("SIM PORT {}", pty.slave_path);
        io::stdout().flush()?;

        let (state, keypair, slot_handles) = load_or_create_state(&state_path, cli_name)?;

        let mut slots = Vec::new();
        for handle in &slot_handles {
            slots.push(SharedNonceSlot(handle.clone()));
        }

        let ab_slots = AbSlots::new(slots);
        let mut signer = FrostSigner::new(keypair, ab_slots);

        for mutation in &state.mutations {
            signer.apply_mutation(mutation.clone());
        }

        let keygen = SimDeviceKeyGen::new(&keypair);
        let rng = rand::thread_rng();

        let held_share = signer.held_shares().next();
        let active_screen = ActiveScreen::new_standby(state.name.as_deref(), held_share);

        Ok(Self {
            pty,
            state,
            state_path,
            signer,
            slot_handles,
            keygen,
            rng,
            upstream_state: UpstreamConnectionState::PowerOn,
            recv_buf: Vec::new(),
            outbox: VecDeque::new(),
            tx_buf: VecDeque::new(),
            active_screen,
            headless,
            firmware_digest,
            pending_device_name: None,
            list_threshold,
        })
    }

    pub fn go_to_default(&mut self) {
        let held_share = self.signer.held_shares().next();
        self.active_screen = ActiveScreen::new_standby(self.state.name.as_deref(), held_share);
    }

    pub fn persist_if_needed(&mut self) -> io::Result<()> {
        let staged = self.signer.staged_mutations();
        self.state.mutations.extend(staged.drain(..));
        update_nonce_slots(&mut self.state, &self.slot_handles);
        save_state_atomic(&self.state_path, &self.state)
    }

    pub fn enqueue_send(&mut self, body: impl Into<DeviceSendBody>) {
        let wire_body: WireDeviceSendBody = body.into().into();
        self.outbox.push_back(DeviceSendMessage {
            from: self.signer.device_id(),
            body: wire_body,
        });
    }

    pub fn enqueue_device_send(&mut self, send: DeviceSend) {
        match send {
            DeviceSend::ToCoordinator(boxed) => {
                self.enqueue_send(DeviceSendBody::Core(*boxed));
            }
            DeviceSend::ToUser(boxed) => {
                self.handle_to_user(*boxed);
            }
        }
    }

    pub fn write_magic_bytes(&mut self) -> io::Result<()> {
        let frame = ReceiveSerial::<Downstream>::MagicBytes(frostsnap_comms::MagicBytes::default());
        let bytes = bincode::encode_to_vec(frame, BINCODE_CONFIG)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.tx_buf.extend(bytes);
        self.drain_tx()
    }

    /// Writes as much of `tx_buf` as the pty accepts without blocking.
    pub fn drain_tx(&mut self) -> io::Result<()> {
        while !self.tx_buf.is_empty() {
            let (front, _) = self.tx_buf.as_slices();
            match self.pty.master.write(front) {
                Ok(0) => break,
                Ok(n) => {
                    self.tx_buf.drain(..n);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn handle_to_user(&mut self, msg: DeviceToUserMessage) {
        match msg {
            DeviceToUserMessage::CheckKeyGen { phase } => {
                self.active_screen = ActiveScreen::new_keygen(*phase);
            }
            DeviceToUserMessage::FinalizeKeyGen { .. } => {
                if let Some(name) = self.pending_device_name.take() {
                    self.state.name = Some(name.clone());
                    if let Err(e) = self.persist_if_needed() {
                        eprintln!("failed to persist device name: {e}");
                    }
                    self.enqueue_send(DeviceSendBody::SetName {
                        name: DeviceName::truncate(name),
                    });
                }
                self.go_to_default();
            }
            DeviceToUserMessage::SignatureRequest { phase } => {
                let rand_seed = self.rng.next_u32();
                self.active_screen =
                    ActiveScreen::new_signing(*phase, rand_seed, self.list_threshold);
            }
            DeviceToUserMessage::SpEcdhRequest { phase } => {
                let rand_seed = self.rng.next_u32();
                self.active_screen =
                    ActiveScreen::new_sp_ecdh(*phase, rand_seed, self.list_threshold);
            }
            DeviceToUserMessage::NonceJobs(mut batch) => {
                batch.run_until_finished(&mut self.keygen);
                // Nonces must be on disk before the coordinator can use them.
                if let Err(e) = self.persist_if_needed() {
                    eprintln!("failed to persist nonces, not answering: {e}");
                    return;
                }
                let response = DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::Signing(DeviceSigning::NonceResponse {
                        segments: batch.into_segments(),
                    }),
                ));
                self.enqueue_device_send(response);
            }
            DeviceToUserMessage::Restoration(restoration) => match *restoration {
                ToUserRestoration::DisplayBackup {
                    key_name: _,
                    access_structure_ref: _,
                    phase,
                } => match phase.decrypt_to_backup(&mut self.keygen) {
                    Ok(backup) => {
                        self.active_screen = ActiveScreen::new_display_backup(&backup);
                    }
                    Err(e) => {
                        eprintln!("unsupported workflow: failed to decrypt backup: {e:?}");
                        self.active_screen = ActiveScreen::new_unsupported("Backup decrypt failed");
                    }
                },
                ToUserRestoration::BackupSaved { .. } => {
                    self.go_to_default();
                }
                other => {
                    eprintln!("unsupported workflow: restoration {:?}", other);
                    self.active_screen = ActiveScreen::new_unsupported("Unsupported workflow");
                }
            },
            other => {
                eprintln!("unsupported workflow: {:?}", other);
                self.active_screen = ActiveScreen::new_unsupported("Unsupported workflow");
            }
        }
    }

    pub fn handle_ui_event(&mut self, event: SimUiEvent) -> io::Result<()> {
        match event {
            SimUiEvent::KeyGenConfirm(phase) => {
                match self
                    .signer
                    .keygen_ack(phase, &mut self.keygen, &mut self.rng)
                {
                    Ok(ack) => {
                        self.persist_if_needed()?;
                        for send in ack {
                            self.enqueue_device_send(send);
                        }
                    }
                    Err(e) => {
                        eprintln!("keygen ack error: {e:?}");
                    }
                }
            }
            SimUiEvent::SigningConfirm(phase) => {
                match self.signer.sign_ack(phase, &mut self.keygen) {
                    Ok(sends) => {
                        self.persist_if_needed()?;
                        for send in sends {
                            self.enqueue_device_send(send);
                        }
                    }
                    Err(e) => {
                        eprintln!("sign ack error: {e:?}");
                    }
                }
            }
            SimUiEvent::SpEcdhConfirm(phase) => {
                match self
                    .signer
                    .sp_ecdh_ack(phase, &mut self.keygen, &mut self.rng)
                {
                    Ok(sends) => {
                        self.persist_if_needed()?;
                        for send in sends {
                            self.enqueue_device_send(send);
                        }
                    }
                    Err(e) => {
                        eprintln!("sp ecdh ack error: {e:?}");
                    }
                }
            }
            SimUiEvent::NameConfirm(new_name) => {
                self.state.name = Some(new_name.clone());
                self.persist_if_needed()?;
                self.enqueue_send(DeviceSendBody::SetName {
                    name: DeviceName::truncate(new_name.clone()),
                });
                self.active_screen = ActiveScreen::new_naming(&new_name);
            }
            SimUiEvent::BackupRecorded => {
                self.enqueue_send(DeviceSendBody::Misc(CommsMisc::BackupRecorded));
                self.go_to_default();
            }
        }
        Ok(())
    }

    pub fn poll_serial(&mut self) -> io::Result<()> {
        self.drain_tx()?;
        let mut buf = [0u8; 1024];
        match self.pty.master.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                self.recv_buf.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                return Err(e);
            }
        }

        match self.upstream_state {
            UpstreamConnectionState::PowerOn => {
                if frostsnap_comms::find_and_remove_magic_bytes::<Upstream>(&mut self.recv_buf)
                    .is_some()
                {
                    self.write_magic_bytes()?;
                    self.enqueue_send(DeviceSendBody::Announce {
                        firmware_digest: self.firmware_digest,
                    });
                    if let Some(name) = &self.state.name {
                        self.enqueue_send(DeviceSendBody::SetName {
                            name: DeviceName::truncate(name.clone()),
                        });
                    } else {
                        self.enqueue_send(DeviceSendBody::NeedName);
                    }
                    self.upstream_state = UpstreamConnectionState::Established;
                }
            }
            UpstreamConnectionState::Established
            | UpstreamConnectionState::EstablishedAndCoordAck => {
                let mut last_message_was_magic_bytes = false;
                loop {
                    if self.recv_buf.is_empty() {
                        break;
                    }
                    match bincode::decode_from_slice::<ReceiveSerial<Upstream>, _>(
                        &self.recv_buf,
                        BINCODE_CONFIG,
                    ) {
                        Ok((received_message, consumed)) => {
                            self.recv_buf.drain(..consumed);
                            last_message_was_magic_bytes =
                                matches!(received_message, ReceiveSerial::MagicBytes(_));

                            match received_message {
                                ReceiveSerial::Message(mut message) => {
                                    let for_me: bool = message
                                        .target_destinations
                                        .remove_from_recipients(self.signer.device_id());
                                    if for_me {
                                        match message.message_body.decode() {
                                            Some(CoordinatorSendBody::AnnounceAck) => {
                                                self.upstream_state =
                                                    UpstreamConnectionState::EstablishedAndCoordAck;
                                            }
                                            Some(CoordinatorSendBody::Naming(naming)) => {
                                                match naming {
                                                    frostsnap_comms::NameCommand::Preview(
                                                        preview_name,
                                                    ) => {
                                                        self.pending_device_name =
                                                            Some(preview_name.to_string());
                                                        self.active_screen =
                                                            ActiveScreen::new_naming(
                                                                preview_name.as_str(),
                                                            );
                                                    }
                                                    frostsnap_comms::NameCommand::Prompt(
                                                        new_name,
                                                    ) => {
                                                        self.active_screen =
                                                            ActiveScreen::new_name_prompt(
                                                                self.state.name.as_deref(),
                                                                new_name.as_str(),
                                                            );
                                                    }
                                                }
                                            }
                                            Some(CoordinatorSendBody::Cancel) => {
                                                self.signer.clear_tmp_data();
                                                self.pending_device_name = None;
                                                self.go_to_default();
                                            }
                                            Some(CoordinatorSendBody::Core(core_message)) => {
                                                match self.signer.recv_coordinator_message(
                                                    core_message,
                                                    &mut self.rng,
                                                ) {
                                                    Ok(sends) => {
                                                        self.persist_if_needed()?;
                                                        for send in sends {
                                                            self.enqueue_device_send(send);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        eprintln!(
                                                            "failed to process core message: {e:?}"
                                                        );
                                                    }
                                                }
                                            }
                                            _ => {
                                                // Ignore upgrade, challenge, data erase, etc.
                                            }
                                        }
                                    }
                                }
                                ReceiveSerial::MagicBytes(_) => {}
                                ReceiveSerial::Conch => {}
                                ReceiveSerial::Reset => {}
                                _ => {}
                            }
                        }
                        Err(bincode::error::DecodeError::UnexpectedEnd { .. }) => {
                            break;
                        }
                        Err(_) => {
                            // Advance one byte on corrupt input
                            if !self.recv_buf.is_empty() {
                                self.recv_buf.remove(0);
                            }
                        }
                    }
                }

                if last_message_was_magic_bytes
                    && self.upstream_state == UpstreamConnectionState::EstablishedAndCoordAck
                {
                    self.upstream_state = UpstreamConnectionState::PowerOn;
                }
            }
        }

        self.flush_outbox()?;
        Ok(())
    }

    pub fn flush_outbox(&mut self) -> io::Result<()> {
        if self.upstream_state < UpstreamConnectionState::Established {
            return Ok(());
        }

        while let Some(msg) = self.outbox.pop_front() {
            self.persist_if_needed()?;

            let frame = ReceiveSerial::<Downstream>::Message(msg);
            let bytes = bincode::encode_to_vec(frame, BINCODE_CONFIG)
                .map_err(|e| io::Error::other(e.to_string()))?;
            self.tx_buf.extend(bytes);
        }

        self.drain_tx()
    }

    pub fn run_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.headless {
            loop {
                self.poll_serial()?;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        } else {
            let display = Rc::new(RefCell::new(SimulatorDisplay::<
                embedded_graphics::pixelcolor::Rgb565,
            >::new(Size::new(240, 280))));
            display.borrow_mut().clear(PALETTE.background)?;

            let output_settings = OutputSettingsBuilder::new()
                .scale(1)
                .pixel_spacing(0)
                .build();

            let title = format!(
                "Frostsnap Device Simulator - {}",
                self.state.name.as_deref().unwrap_or("unnamed")
            );
            let mut window = Window::new(&title, &output_settings);
            {
                let mut target = SuperDrawTarget::from_shared(display.clone(), PALETTE.background);
                self.active_screen
                    .draw(&mut target, Instant::from_millis(0));
            }
            window.update(&display.borrow());

            let mut last_screen = std::mem::discriminant(&self.active_screen);
            let mut last_touch: Option<Point> = None;
            let mut drag_start: Option<Point> = None;
            let mut is_dragging = false;
            const DRAG_THRESHOLD: i32 = 5;
            let start_time = SystemTime::now();

            'running: loop {
                let elapsed = SystemTime::now()
                    .duration_since(start_time)
                    .unwrap_or_default();
                let current_time = Instant::from_millis(elapsed.as_millis() as u64);

                for event in window.events() {
                    match event {
                        SimulatorEvent::Quit => break 'running,
                        SimulatorEvent::MouseButtonDown { point, .. } => {
                            drag_start = Some(point);
                            last_touch = Some(point);
                            self.active_screen.handle_touch(point, current_time, false);
                            is_dragging = false;
                        }
                        SimulatorEvent::MouseButtonUp { point, .. } => {
                            if is_dragging {
                                let prev_y = last_touch.map(|p| p.y as u32);
                                self.active_screen.handle_vertical_drag(
                                    prev_y,
                                    point.y as u32,
                                    true,
                                );
                            } else {
                                self.active_screen.handle_touch(point, current_time, true);
                            }
                            drag_start = None;
                            last_touch = None;
                            is_dragging = false;
                        }
                        SimulatorEvent::MouseMove { point } => {
                            if let Some(start) = drag_start {
                                let distance = (((point.x - start.x).pow(2)
                                    + (point.y - start.y).pow(2))
                                    as f32)
                                    .sqrt() as i32;

                                if distance > DRAG_THRESHOLD {
                                    if !is_dragging {
                                        is_dragging = true;
                                    }
                                    let prev_y = last_touch.map(|p| p.y as u32);
                                    self.active_screen.handle_vertical_drag(
                                        prev_y,
                                        point.y as u32,
                                        false,
                                    );
                                }
                                last_touch = Some(point);
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(ui_event) = self.active_screen.poll() {
                    self.handle_ui_event(ui_event)?;
                }

                if self.active_screen.is_finished() {
                    self.go_to_default();
                }

                self.poll_serial()?;

                // Widgets only redraw what changed, so a new screen would be drawn over the
                // previous one. Clear the display when the screen changes.
                let screen = std::mem::discriminant(&self.active_screen);
                if screen != last_screen {
                    display.borrow_mut().clear(PALETTE.background)?;
                    last_screen = screen;
                }

                {
                    let mut target =
                        SuperDrawTarget::from_shared(display.clone(), PALETTE.background);
                    self.active_screen.draw(&mut target, current_time);
                }

                window.update(&display.borrow());
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            Ok(())
        }
    }
}
