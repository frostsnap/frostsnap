# esp-hal-1.2 staging branch manifest

Branch `esp-hal-1.2` in worktree `~/src/fswt/esp-hal-1.2` (not pushed). Base: `master` `eb2356f5`.
Source snapshot: `esphal-v1-restack` at `3538e5e4` (was `f24e7758`; moved by one commit, see below).

## Commits

| # | sha | author (+co-author) | subject |
|---|-----|---------------------|---------|
| 1 | 78eb947e | Adam Mashrique (+LLFourn) | [deps,nix] Move to Rust 1.95 |
| 2 | 0e9d8e1b | LLFourn (+Adam Mashrique) | [device,widgets] Reduce the largest stack frames |
| 3 | f425207f | LLFourn (+Adam Mashrique) | [device] Upgrade to esp-hal 1.2 |
| 4 | aef6e71f | Adam Mashrique (+LLFourn) | [device] Read the partition table with esp-bootloader-esp-idf |
| 5 | 5d92dbbc | Adam Mashrique (+LLFourn) | [factory,nix] Build the bootloader from ESP-IDF v5.5.4 |
| 6 | 890e5fba | Adam Mashrique | [device] Remove unused cfg_example.toml |

## Decisions recorded

- Commit 1 stays separate: master's esp-hal 0.22 code passes `lint-device` and builds both boards on
  Rust 1.95 (only a pre-existing future-incompat note for `proc-macro-error2`). Content:
  `rust-toolchain.toml` and `flake.lock` from `f24e7758`; `f134a58f` cherry-picked (auto-merged
  cleanly into master's `frosty_ui.rs`).

- Commit 2: `15358b9d`, `d49b60c2`, `7e1e036e` cherry-picked. `15358b9d` conflicted with master's
  pre-port `FrostyUi::new(display, touch_receiver, timer)` signature; resolved by keeping master's
  signatures and adding the `#[inline(never)]` / `Box::new` on top. `15358b9d`'s
  `#[inline(never)]` on `Partitions::load` is moved to commit 4: its comment is about the
  esp-bootloader-esp-idf read buffer, which does not exist until then.
- Commit 3 is built from the source tree, not by replaying port commits: every path that differs
  between `eb2356f5` and `f24e7758` (excluding `.clank/`) is taken from `f24e7758`, then the pieces
  owned by commits 4–6 are restored to master (`device/src/partitions.rs`, `device/.cargo/config.toml`,
  the root `.cargo/config.toml` is not added, `esp-partition-table = "0.1"` stays in
  `device/Cargo.toml`, `frostsnap_factory/bootloader/flake.nix`, `PROVISIONING.md`,
  `sdkconfig.defaults.dev`, `device/cfg_example.toml` kept). `Cargo.lock` is the source lock plus
  cargo re-adding exactly `esp-partition-table 0.1.3` and `md5 0.7.0` (17 lines, nothing else moved).
  So commits 3–6 compose back to the source tree by construction. `frostsnap_core` unchanged.
- Source snapshot moved `f24e7758` → `3538e5e4`: the source's `frostsnap_factory/bootloader/flake.nix`
  carried a FIXME (from `048bae89`) saying our images lack an app descriptor, so a v5.5.4 bootloader
  would reject them. That is stale: the built frontier and legacy ELFs both have a 256-byte
  `.flash.appdesc` at `0x3c000020` (start of DROM) holding magic `0xABCD5432`. Rather than hand-edit
  on staging, the comment was fixed on `esphal-v1-restack` (`3538e5e4`, untagged: no plan's files)
  and staging takes the fixed file.
- Commit 4 takes `partitions.rs`, both `.cargo/config.toml`s, `device/Cargo.toml` and `Cargo.lock`
  from the source, including `15358b9d`'s `#[inline(never)]` on `Partitions::load`. Available stack
  falls 28500 → 28156 B at this commit (the new parser).
- Tree equality: `git diff --stat 3538e5e4 890e5fba -- . ':!.clank' ':!staging'` is empty.
  `frostsnap_core` is unchanged from `eb2356f5` at every commit; no commit contains `.clank/` or
  `staging/`; no subject carries a plan name.

## Verification per commit

| # | lint-device | build legacy+frontier | stack-check (largest frame, % of available) |
|---|-------------|------------------------|---------------------------------------------|
| 1 | pass | pass | pass: `NumericKeyboard::new` 7872 B, 24.3% of 32332 B |
| 2 | pass | pass | pass: `DeviceLoop::poll` 6432 B, 19.9% of 32332 B |
| 3 | pass | pass | pass: `DeviceLoop::poll` 6416 B, 22.5% of 28500 B |
| 4 | pass | pass | pass: `DeviceLoop::poll` 6416 B, 22.8% of 28156 B |
| 5 | (= 6 for firmware: factory files only) | ″ | ″ |
| 6 (tip) | pass | pass | pass: `DeviceLoop::poll` 6416 B, 22.8% of 28156 B |

Tip only: `just gen` pass; `lint-ordinary` pass; `test-ordinary --release --all-features --locked` pass (253 passed, 0 failed, 49 suites).
