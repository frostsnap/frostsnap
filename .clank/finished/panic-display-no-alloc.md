# panic-display-no-alloc
# The panic handler must not allocate for its display buffer

## Rules for anything that leaves this machine

- Commit in this worktree (`esphal-v1-restack`, head of draft PR frostsnap/frostsnap#601). Pushing
  to `fork` (`LLFourn/frostsnap`) branch `esphal-v1-restack` is allowed; never push to `origin`, never
  touch `esphal-v1-attempt-1`. No GitHub comments, reviews or PR-body edits.
- No `claude.ai/code` link and no `Claude-Session:` trailer in any commit. Do not sign commits.
- `frostsnap_core` must stay byte-identical to `master` (`eb2356f5`).

## Why

The port moved from mipidsi 0.8 + `display-interface-spi` to mipidsi 0.10, whose `SpiInterface`
takes a caller-supplied pixel batching buffer. `init_display!` (`device/src/peripherals.rs:~55`)
makes it with `Box::leak(Box::new([0u8; 512]))`, and `handle_panic` (`device/src/panic.rs:36`) uses
the same macro, so the panic path now allocates. Master's panic path did not. If the heap is
exhausted (fewer than 512 contiguous bytes free), an out-of-memory panic re-enters the allocator
from the panic handler and the error screen never draws.

## Decided — build this, do not relitigate it

1. `init_display!` takes the buffer as a parameter.
2. `DevicePeripherals::init` passes the leaked 512-byte heap buffer: the display is returned out of
   `init` and lives for the life of the firmware, so the buffer must be `'static`.
3. `handle_panic` passes a 64-byte array on its own stack: it draws one error screen, so batching
   speed is irrelevant (mipidsi only requires the buffer to hold at least one pixel).
4. While there: `panic.rs` uses `peripherals.GPIO1` from the `Peripherals::steal()` it already
   does, instead of a second `GPIO1::steal()`.

## Out of scope

- Anything else in the panic handler or display setup.

## Milestones

1. All of the above (one commit).

## Verification

- `RUSTUP_TOOLCHAIN=stable just lint-device --release --locked`,
  `just build-firmware legacy --locked && just build-firmware frontier --locked`,
  `just stack-check stacks --board frontier --max-pct 25` (after
  `source ~/src/fs-war-room/tools/frostsnap-env.sh`); report `handle_panic`'s frame before/after.
- Hardware (LLFourn; not a gate for FINISHED): a forced panic still draws the error screen.
