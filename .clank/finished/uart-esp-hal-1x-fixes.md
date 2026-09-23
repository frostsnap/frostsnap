# uart-esp-hal-1x-fixes
# Fix the device UART regressions the esp-hal 1.2 port introduced

## Rules for anything that leaves this machine

- Commit in this worktree (`esphal-v1-restack`, head of draft PR frostsnap/frostsnap#601). Pushing
  to `fork` (`LLFourn/frostsnap`) branch `esphal-v1-restack` is allowed; never push to `origin`, never
  touch `esphal-v1-attempt-1`. No GitHub comments, reviews or PR-body edits.
- No `claude.ai/code` link and no `Claude-Session:` trailer in any commit. Do not sign commits.
- `frostsnap_core` must stay byte-identical to `master` (`eb2356f5`).

## Why

The port moved the device from esp-hal 0.22 to 1.2.1 and kept our UART design: each `Uart<Blocking>`
lives in a `static Mutex<RefCell<Option<Uart>>>` (`device/src/uart_interrupt.rs`), a `#[handler]`
drains both UARTs into lock-free queues and clears `RxFifoFull`, and TX goes through the same mutex.
That design is esp-hal 1.2's own documented blocking pattern (the example on `Uart::listen`,
esp-hal-1.2.1 `src/uart/mod.rs` ~1600-1650) and stays. But the 1.2 driver calls underneath it changed
meaning, and the port did not adapt, so the device-to-device wire path now behaves differently from
shipped firmware:

1. **RX overflow loses the whole FIFO, silently.** 0.22's blocking `read_byte` (fork
   `esp-hal/src/uart.rs:803-821`) only checked `rxfifo_cnt` and never looked at error flags, so an
   overflow lost only the bytes that arrived while the FIFO was full. 1.2's `read_buffered`
   (`src/uart/low_level/mod.rs:814-834`) checks errors *before* reading, returns `Err` without
   reading, and on `FifoOvf` resets the RX FIFO (`check_for_errors_and_reset_fifo`, `:741-753`),
   dropping up to 128 buffered bytes. `drain_uart_to_queue` does `if let Ok(n) = …` and discards the
   error, so the loss surfaces later as a protocol decode failure far from its cause.
   `RxConfig::reported_errors` defaults to `EnumSet::all()` (`src/uart/mod.rs:456`), so frame,
   parity and glitch errors also return `Err`; for those the FIFO is untouched and the bytes are read
   on the next call, so dropping them loses nothing, but it does end the drain loop early.
2. **A blocking flush runs with interrupts masked.** `UartHandle::flush_tx` calls 1.2's
   `Uart::flush()` (`src/uart/mod.rs:1005-1019`: spin until `tx_fifo_count()==0`, 10 µs, spin until
   the TX FSM is idle) inside `critical_section::with`. 0.22's `flush_tx` was a non-blocking poll, and
   `SerialIo::flush` (`device/src/io.rs:312-325`) still loops on `WouldBlock`, which can no longer
   happen. Interrupts, including the other UART's RX ISR, are masked for as long as the TX FIFO takes
   to drain: at most ~66 ms at 19200 baud with a full FIFO, typically a few ms. Callers: every
   `SerialIo::change_baud` (OTA entry `ota.rs:392-394`, exit `:480-482`), `send_reset_signal`, and
   the end-of-OTA `downstream_io.flush()` (`ota.rs:475`).

Neither changes on-device data or the wire format; both change wire *behaviour* under load.

## Decided — build this, do not relitigate it

1. **Keep the blocking ISR design.** No async driver, no UHCI/DMA, no RX/TX `split()`: interrupt
   control and baud changes exist only on the whole `Uart` in 1.2 (`Uart::listen`,
   `clear_interrupts`, `set_interrupt_handler`, and `Uart::apply_config` is the only place baud is
   set; `UartRx/UartTx::apply_config` touch only thresholds).
2. **Overflow panics the device.** `drain_uart_to_queue` matches the `RxError`: `FifoOverflowed`
   sets a per-UART flag (the ISR must not allocate or panic), other errors are ignored, and every
   error case keeps draining — an `Err` is not "no data". One task-context function checks both
   UARTs' flags and panics naming the UART; it runs at the top of every `DeviceLoop::poll` (so an
   idle or disconnected port that is never read again still panics) and in `SerialIo::read_byte`
   (so the blocking OTA loop and decoding panic before the gap surfaces as a decode failure, reset
   or park). No `DeviceSendBody::Debug` reporting. (A non-fatal on-screen connection warning
   would be nicer, but that is a separate UI plan.) Set `reported_errors` to overflow only, so
   frame/parity/glitch flags stop producing `Err` at all.
3. **Read in chunks.** Each `read_buffered` call reads up to a stack chunk (32 or 64 bytes) and
   enqueues all of it; no more one-byte calls.
4. **Flush without the critical section.** Waiting for TX to drain reads only side-effect-free
   status registers, so it needs no exclusion from the RX ISR. Implement `tx_drained()` from the
   UART's registers (`UART0::regs()` / `UART1::regs()`, the same access `efuse.rs` already uses via
   `EFUSE::regs()`): `status().txfifo_cnt() == 0`, then the 10 µs settle, then
   `fsm_status().st_utx_out() == 0` — mirroring 1.2's own `flush`/`flush_last_byte` and `low_level/v1.rs:86-92` (the C3 is `uart_version = "1"`)
   `is_tx_idle`. `flush_tx` becomes a plain blocking `fn flush_tx(&mut self)` built on it; delete the
   dead `WouldBlock` loop and the `nb::Result` return. The critical section is taken only where the
   `Uart` itself is touched (`write`, `apply_config`).
5. **One source for the UART config.** A single function builds the `uart::Config` for a baud rate
   (baud + `RxConfig` threshold + `reported_errors`). Construct each UART in `peripherals.rs` with the
   `BAUDRATE` config directly instead of `Config::default()` followed by a re-apply in
   `SerialInterface::new_uart`; `UartHandle::change_baud` uses the same function.
6. **Delete the unreachable `written == 0` branch** in `UartHandle::write_bytes`.

## Out of scope

- The I2C/touch timeout and the GPIO-handler-under-`GPIO_LOCK` latency (separate plan).
- Any wire-format or protocol change; OTA handshake changes.
- #513's runtime lift. Keep the diff local to `uart_interrupt.rs`, `io.rs` and the UART block of
  `peripherals.rs` so that rebase stays mechanical.

## Milestones

1. **RX path**: decisions 2, 3 and the `reported_errors` part of 5. Commit.
2. **TX flush**: decision 4 and 6. Commit.
3. **Config source**: decision 5. Commit.

## Verification

- Each milestone: `RUSTUP_TOOLCHAIN=stable just lint-device --release --locked`,
  `just build-firmware legacy --locked && just build-firmware frontier --locked`,
  `just stack-check stacks --board frontier --max-pct 25` (run with
  `source ~/src/fs-war-room/tools/frostsnap-env.sh`). None may regress; report the stack-check top
  frame before/after.
- Reviewers: confirm against esp-hal-1.2.1 source that `tx_drained()` reads the same registers
  1.2's `flush` does on the C3, that no path still calls `Uart::flush()` under a critical section,
  and that no RX error path stops draining.
- Hardware (LLFourn, before #601 merges; not a gate for FINISHED): OTA-update a chain of ≥2
  frontier devices, and plug/unplug a device mid-chain while the app is talking to it; neither may hit
  the overflow panic in normal use.
