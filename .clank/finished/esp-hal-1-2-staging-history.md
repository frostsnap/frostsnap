# esp-hal-1-2-staging-history
# Rebuild the esp-hal 1.2 port as a few clean commits on a new staging branch

## Rules for anything that leaves this machine

- The staging branch is built in its own worktree (`~/src/fswt/esp-hal-1.2`, branch `esp-hal-1.2`)
  and is NOT pushed anywhere; LLFourn decides when and where. Never push to `origin`; never touch
  `esphal-v1-attempt-1`. `esphal-v1-restack` (#601) is only read from, never rewritten.
- No GitHub comments, reviews or PR-body edits.
- No `claude.ai/code` link and no `Claude-Session:` trailer in any commit. Do not sign commits.

## Why

`esphal-v1-restack` (draft #601) records the port as it happened: "port to 1.0", "upgrade to 1.1",
"upgrade to 1.2", intermediate fixes, stack-budget commits, then clank-managed regression fixes with
plan-name subjects and `.clank/` files. LLFourn wants the landing history to be a few neat commits: a
single "upgrade to esp-hal 1.2", with anything peripheral in its own commit. This plan manages that
other branch; this worktree only carries the plan and a manifest for review.

## The invariant

The staging tip's tree equals `esphal-v1-restack`'s tree at the source snapshot, minus `.clank/` and
minus `staging/`:
`git diff --stat <source-sha> esp-hal-1.2 -- . ':!.clank' ':!staging'` is empty. The source snapshot
is `esphal-v1-restack`'s head when this plan is promoted (all earlier plans finished); record the sha
in the manifest. Nothing is re-implemented by hand: every commit's content comes from the source
branch, only regrouped.

## Decided — build this, do not relitigate it

Base: `master` `eb2356f5`. Target commits, in this order:

1. **Rust 1.95 toolchain** (`[deps,nix]`): `rust-toolchain.toml` 1.95.0, the `flake.lock`
   rust-overlay bump, and the 1.95 clippy fixes (`f134a58f`: `frosty_ui.rs` collapsible match,
   `touch_handler.rs`). **Only if master's esp-hal 0.22 code lints and builds on 1.95**; if it does
   not, fold this into commit 3 (the upgrade: esp-hal 1.2.1 itself requires Rust 1.95, so the
   toolchain must land no later than it) and say so in the manifest.
2. **Stack frame reductions** (`[device,widgets]`): Box `Resources.ui` and outline `FrostyUi::new` /
   `Partitions::load` (`15358b9d`), box each `NumericKeyboard` row (`d49b60c2`, with the
   `KeypadRows` alias), `#[inline(never)]` on `CheckBackupScreen::draw` (`7e1e036e`). These are
   valid refactors on master's code, so they land *before* the upgrade and every commit passes the
   stack gate. The message states they make room for esp-hal 1.2, which leaves less stack.
3. **Upgrade to esp-hal 1.2** (`[device]`): everything else in `device/`, `cst816s/`,
   `frostsnap_widgets/Cargo.toml`, workspace `Cargo.toml`/`Cargo.lock` for the port: the 1.x API
   port, `timg` Timer → `esp_hal::time::Instant`, DS signing without the fork, efuse free functions,
   `esp-bootloader-esp-idf` app descriptors in all three binaries, SPI at 80 MHz as on master, and the
   regression fixes found in review (UART overflow panics / TX drain outside the critical section /
   one `uart_config`; touch I2C `BusCycles(10)`; panic display buffer on the stack). Those fixes are
   folded in because they correct the upgrade itself: no commit in the landed history should carry a
   regression that a later commit repairs. The message explains the behaviour-relevant points
   (overflow now panics; UART/I2C timing restored to 0.22's). `TODO.md` never appears.
4. **Partition table via `esp-bootloader-esp-idf`** (`[device]`): the parser swap (`07a4e020`,
   `2347d29a`'s partitions hunk), the boxed read buffer (`13cec0b6`), and the
   `ESP_BOOTLOADER_ESP_IDF_CONFIG_PARTITION_TABLE_OFFSET` config.
5. **ESP-IDF v5.5.4 bootloader** (`[factory,nix]`): `frostsnap_factory/bootloader/flake.nix`
   (`6d9c8ecc`, `048bae89`), `PROVISIONING.md`, `sdkconfig.defaults.dev` log colours (`137efecf`).
   Must come after commit 3: v5.4+ validates the app descriptor, which master's firmware lacks.
6. **Remove unused `device/cfg_example.toml`** (`[device]`), if not better placed elsewhere.

Authorship: keep credit. Each commit's author is whoever wrote most of it; everyone else who
contributed gets a `Co-authored-by:` trailer (Adam Mashrique
`<9629456+musdom@users.noreply.github.com>`, LLFourn `<lloyd.fourn@gmail.com>`). Commit 3 is
LLFourn with Adam as co-author; commit 5 is Adam with LLFourn as co-author.

If a cut above cannot be made cleanly (a hunk needs code from a later commit), move the smallest
necessary piece into the earlier commit and record why in the manifest; do not reorder the list
without saying so.

## How review works here

Each milestone commits (tagged with this plan) a manifest `staging/esp-hal-1.2.md` in THIS worktree,
recording the staging branch's commit list and shas, and each commit's verification results.
Reviewers review the staging branch at those shas (`git -C ~/src/fswt/esp-hal-1.2 show <sha>`).
`staging/` never goes onto the staging branch.

## Milestones

1. Worktree and branch from `eb2356f5`; commit 1 (or the recorded decision to fold it).
2. Commit 2.
3. Commit 3.
4. Commits 4–6, and the tree-equality check against the source snapshot.

## Verification

- Every staging commit: `RUSTUP_TOOLCHAIN=stable just lint-device --release --locked`,
  `just build-firmware legacy --locked && just build-firmware frontier --locked`,
  `just stack-check stacks --board frontier --max-pct 25` (`source ~/.bash_profile` first; do not
  silence it). Record pass/fail and the top frame for each in the manifest.
- Staging tip: also `RUSTUP_TOOLCHAIN=stable just lint-ordinary --release --locked` and
  `RUSTUP_TOOLCHAIN=stable just test-ordinary --release --all-features --locked` (after `just gen`).
- The tree-equality invariant holds, and `frostsnap_core` is identical to `master` at every commit.
- No commit subject carries a plan name; no commit contains `.clank/` or `staging/`.
