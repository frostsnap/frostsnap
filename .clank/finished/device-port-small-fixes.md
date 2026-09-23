# device-port-small-fixes
# Restore the touch controller's I2C bus timeout the port dropped

## Rules for anything that leaves this machine

- Commit in this worktree (`esphal-v1-restack`, head of draft PR frostsnap/frostsnap#601). Pushing
  to `fork` (`LLFourn/frostsnap`) branch `esphal-v1-restack` is allowed; never push to `origin`, never
  touch `esphal-v1-attempt-1`. No GitHub comments, reviews or PR-body edits.
- No `claude.ai/code` link and no `Claude-Session:` trailer in any commit. Do not sign commits.
- `frostsnap_core` must stay byte-identical to `master` (`eb2356f5`).

## Why

esp-hal 0.22's I2C default had a bus timeout and shipped firmware ran with it; the port to 1.2
silently dropped it, while 1.2 also started running the touch read with interrupts masked. This plan
covers only the touch controller. (Scope was cut back from a broader small-fixes plan: the panic
display buffer, efuse ownership, legacy app descriptor and partition-offset items are separate
plans.)

## Decided — build this, do not relitigate it

1. **Restore the touch I2C bus timeout.** esp-hal 0.22's `I2cConfig::default()` had
   `timeout: Some(10)` (SCL unchanged for more than 10 bus cycles aborts the transfer; fork
   `esp-hal/src/i2c/master/mod.rs:244-266`), and shipped firmware ran with it. 1.2's C3 default is
   `BusTimeout::Disabled` (esp-hal-1.2.1 `src/i2c/master/mod.rs:630-640`), and the port only sets the
   frequency (`device/src/peripherals.rs:237-243`). 1.2 also calls the GPIO handler — where
   `cst816s` does this I2C read — under `GPIO_LOCK` with interrupts masked
   (`src/gpio/interrupt.rs:163-177`), so a stalled bus now holds interrupts off until the much longer
   FSM timeout. Match shipped behaviour: `.with_timeout(BusTimeout::BusCycles(10))`
   (`esp_hal::i2c::master::BusTimeout`; 1.2 rounds it to a power of two, so the effective value may
   be up to ~2x). Nothing else about the touch path changes; moving the I2C read out of the ISR is a
   separate question.

## Out of scope

- Moving the touch I2C read out of the GPIO ISR.
- Everything else from the port review (panic display buffer, efuse ownership, legacy app
  descriptor, partition offset): separate plans.

## Milestones

1. The timeout (one commit).

## Verification

- `RUSTUP_TOOLCHAIN=stable just lint-device --release --locked`,
  `just build-firmware legacy --locked && just build-firmware frontier --locked`,
  `just stack-check stacks --board frontier --max-pct 25` (after
  `source ~/src/fs-war-room/tools/frostsnap-env.sh`).
- Hardware (LLFourn; not a gate for FINISHED): touch works normally on frontier and legacy.
