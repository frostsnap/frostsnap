# remote_features_dev_gate
# Remote (nostr) feature surface exists only in developer mode

The remote-wallet feature set isn't ready for regular users; its
entry points should not exist in the UI unless developer mode is on.
Gate the ENTRY surfaces only — wallets that are already remote keep
working (the shell gate is `coordination_ui_enabled`, untouched, and
Group Info's "Leave remote wallet" stays as the way out).

## Surfaces to gate (inventory)

1. **Homepage "Join with invite link" card** (`wallet_add.dart`,
   `WalletAddColumn`) — remote-only feature; hide the card. This
   surface lives on the long-lived homepage, so gate REACTIVELY via
   the existing pattern: `StreamBuilder<DeveloperSettings>` on
   `settingsCtx.developerSettings` (see `_DevUsbOverlay` in
   `main.dart`) — toggling dev mode updates the homepage without a
   restart.
2. **Create fork** — `showWalletCreateDialog` currently always opens
   `OrgKeygenPage` ("Who is this for?": Just me / A group of us).
   Dev off: skip the chooser entirely and open `WalletCreatePage`
   directly (same unplug + open tail). Dev on: unchanged.
3. **Restore fork** — `showRestoreChooserDialog` ("Where are the
   keys?": with your devices / with others). Dev off: skip the
   chooser and go straight to `showWalletRecoverDialog` (local).
   Dev on: unchanged.
4. **"Coordinate over Nostr" tile** in the wallet More sheet
   (`wallet_more.dart`) — render nothing when dev off (the file
   already reads `isInDeveloperMode` for other tiles; note the
   tile-group corner shapes may need the same adjustment those
   tiles already do).
5. **"Coordinate over Nostr" switch** in wallet settings
   (`settings.dart` ~192) — hide when dev off.
6. **"Nostr profile" settings entry** (`settings.dart` ~252) — the
   in-channel identity exists only for remote coordination; hide
   when dev off.

Dialog-opened surfaces (2, 3) read
`SettingsContext.of(context)?.settings.isInDeveloperMode() ?? false`
synchronously at open time — the established pattern for one-shot
flows. Only the persistent homepage needs the stream.

## Deliberately NOT gated

- Deep-link handling (`frostsnap://` → join dialog): clicking an
  invite link is a deliberate act, not browsable surface; joining
  still works with dev mode off. (If review disagrees, gating it is
  one `if` at the dispatch site — but the plan's default is
  functional.)
- The remote shell, chat, Members page, and Group Info for wallets
  ALREADY in remote mode — including "Leave remote wallet".
- `EnterWalletNameView`'s network chooser and other existing
  dev-mode gates — already correct.

## Tests

- `restore_chooser` flow: with dev off the chooser is skipped —
  cover at whatever seam avoids a live SettingsContext; if the
  static dialog methods resist testing, extract the tiny
  `devMode ? chooser : local` decision into a pure function and
  test that.
- Homepage: widget test that the Join card is absent when
  `DeveloperSettings(developerMode: false)` streams in and present
  when true, if a fake SettingsContext is feasible; otherwise the
  decision-seam test carries it.
- Existing suites stay green.

## Acceptance

- `flutter analyze` + Dart suites green.
- Manual (user): dev mode OFF → homepage shows Create/Restore only;
  Create goes straight to device keygen; Restore goes straight to
  the local flow; no nostr tiles in wallet More/settings; an
  existing remote wallet still opens with chat. Dev mode ON →
  everything as today.
