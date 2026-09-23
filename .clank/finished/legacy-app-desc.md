# legacy-app-desc
# Give the legacy binary an ESP-IDF app descriptor

## Rules for anything that leaves this machine

- Commit in this worktree (`esphal-v1-restack`, head of draft PR frostsnap/frostsnap#601). Pushing
  to `fork` (`LLFourn/frostsnap`) branch `esphal-v1-restack` is allowed; never push to `origin`, never
  touch `esphal-v1-attempt-1`. No GitHub comments, reviews or PR-body edits.
- No `claude.ai/code` link and no `Claude-Session:` trailer in any commit. Do not sign commits.
- `frostsnap_core` must stay byte-identical to `master` (`eb2356f5`).

## Why

The port added `esp_bootloader_esp_idf::esp_app_desc!();` to `device/src/bin/frontier.rs` and
`widget_dev.rs` but not `legacy.rs`. espflash ≥ 4 (what `just legacy-flash`'s cargo runner uses)
rejects images without an app descriptor, and the three binaries should be built the same way. The
legacy bootloader (`device/bootloader-legacy.bin`, ESP-IDF v5.1.6) does not validate the descriptor,
so existing legacy devices are unaffected.

## Decided — build this, do not relitigate it

1. Add `esp_bootloader_esp_idf::esp_app_desc!();` to `device/src/bin/legacy.rs`, placed as in
   `frontier.rs`.

## Milestones

1. That (one commit).

## Verification

- `RUSTUP_TOOLCHAIN=stable just lint-device --release --locked`,
  `just build-firmware legacy --locked && just build-firmware frontier --locked`,
  `just stack-check stacks --board frontier --max-pct 25` (`source ~/.bash_profile` first; do not
  silence it).
- Hardware (LLFourn; not a gate for FINISHED): a legacy device flashed via `just legacy-flash` boots.
