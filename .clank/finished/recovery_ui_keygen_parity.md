# recovery_ui_keygen_parity
# Recovery create + lobby match the keygen/local-recovery patterns

Two UX findings from the user's walkthrough of the redesigned
remote-recovery ceremony:

1. **Create form crams everything onto one page.** Local recovery
   splits wallet-name and threshold onto separate dialog pages, and
   its threshold page is a *choice* ("I'm not sure" vs "I know the
   threshold" radio cards), not a bare optional number field. That's
   better — and the code already exists as reusable views.
2. **The lobby reads bottom-up.** The status card ("Waiting for key
   shares — N of M") sits below the participant list; keygen puts
   status at the top and the participant list (with Invite
   participants) at the bottom. Keygen's participant *cards* are
   also better than recovery's plain rows, and its contribute
   affordance placement is right: the footer primary button until
   you've contributed, then an affordance on your own card.

## Design direction (frontend-design pass)

- **The lobby is a status board, not a form.** Top-to-bottom
  narrative: title (wallet name) → **status headline** (what the
  ceremony needs right now: "Waiting for key shares — 1 of 2" /
  "Recovery available") → who's here and what they gave
  (participant cards) → how to grow the room (Invite tile) →
  one primary action in the footer.
- **One phase-aware primary button**, exactly like keygen's
  `_LobbyPrimaryButton`: it always tells you the single most useful
  thing you can do — "Load key share" before you've contributed;
  then for the leader "Recover" (enabled when a recovery is
  available, disabled "Waiting for key shares" otherwise) and for
  joiners a disabled "Waiting for recovery".
- **Cards speak the keygen dialect**: `Card.filled` on
  `surfaceContainerHigh`, avatar with a badge on the leader (like
  keygen's Host badge), status text on the right ("Waiting for
  you" for self pre-contribution, "Joined", green "Ready" once
  posted), chevron-expandable sub-list of contributed key shares
  (key icon + device name — keygen's `_DeviceList` shape).
- **Self-service lives on your own card**: once you've posted a
  share, a compact `+` icon on your own card posts another
  (mirroring keygen's edit-pencil placement). The standalone
  load-share tile and the floating "Add another key share" button
  disappear.

## Task 1 — stepped create flow reusing local recovery's views

`RemoteRecoveryPage.create` currently shows `CreateLobbyForm` (one
step: name + threshold text field + network). Replace with two
steps that REUSE the restoration views (`restoration/
enter_wallet_name_view.dart`, `restoration/enter_threshold_view.dart`)
— same widgets local recovery renders, driven the same way
(GlobalKey + `currentState!.submit()`, footer button in the
`MultiStepDialogScaffold`, mirroring `recovery_flow.dart`'s
`_buildEnterRestorationDetailsStep` / `_buildEnterThresholdStep`):

- Step `walletName`: `EnterWalletNameView` (name +
  `BitcoinNetworkChooser` behind its existing developer-mode gate —
  replaces the always-visible `NetworkAdvancedOptions`). Copy for
  the remote context ("The name of the wallet being recovered…")
  can ride the step subtitle; the view's own intro text is
  restoration-specific, so add an optional `intro` override param
  to the view if the text reads wrong — do NOT fork the widget.
- Step `threshold`: `EnterThresholdView` verbatim — the "I'm not
  sure" (→ `thresholdHint = null`) vs "I know the threshold"
  radio-card selector. Back navigates to the name step.
- On threshold submit → assemble `CreateLobbyResult` → existing
  `_submitCreate` path (identity gate → `dispatchCreate` →
  cross-fade to lobby). `CreateLobbyForm` is deleted.

## Task 2 — lobby layout to keygen parity

Rework `RecoveryLobbyView` (pure view stays pure; all data already
on the snapshot):

- **Order**: status block at top → 'Participants' header with the
  posted-count summary → keygen-style participant cards →
  `InviteTile` (leader, while live) → banners (finished/cancelled/
  error) → footer.
- **Participant cards**: expandable `Card.filled` rows per the
  design direction. Leader badge requires knowing the leader's
  pubkey: add `leader: Option<PublicKey>` to `RecoveryLobbyState`
  (the fold already reads the creation event's author for Finish/
  Cancel authorization — set the field when the fold is created;
  one-line mirror addition). This matches keygen's
  `LobbyState.initiator` precedent.
- **Footer**: left slot keeps Cancel lobby / Leave lobby / Close
  exactly as-is; the right slot becomes the phase-aware primary
  (replacing the always-Recover leader button):
  - `snapshot == null` → disabled "Connecting…"
  - I haven't posted a share → "Load key share" → `onLoadShare`
  - leader, recovery available → "Recover"
  - leader, otherwise → disabled "Waiting for key shares"
  - joiner, posted → disabled "Waiting for recovery"
- **Own card `+`**: post-contribution affordance for additional
  shares, trailing-slot placement per keygen's edit pencil.
- Delete `_LoadShareTile` and the "Add another key share"
  TextButton.

## Deliberately NOT done

- No transport changes beyond the `leader` field on
  `RecoveryLobbyState` (+ mirror + regen).
- No keygen-side changes — it's the reference, not a target.
- No changes to the reused restoration views beyond an optional
  intro-text param (and only if the default copy reads wrong in
  the remote context).
- Local recovery flow untouched.

## Tests

- `recovery_create_page_test.dart`: rework to the stepped flow —
  `EnterThresholdView` is context-free, so drive it directly for
  the "I'm not sure" → null and "I know" → N assembly into
  `CreateLobbyResult`; the `dispatchCreate` stub-capture test is
  unchanged. `EnterWalletNameView` requires `SettingsContext`
  (dev-mode gate), and its behavior is already exercised by local
  recovery — don't stand up FFI settings in tests just to re-cover
  it; assert the step wiring at the result-assembly seam instead.
- `recovery_lobby_view_test.dart`: adapt to the new order and
  footer phases (each footer state above gets a case), leader
  badge from `state.leader`, own-card `+` fires `onLoadShare`,
  expandable card shows device names.
- `cargo test -p frostsnap_nostr` green (leader-field addition
  touches the fold tests' constructors).

## Acceptance

- `flutter analyze` + `dart format` clean; all recovery widget
  tests + `cargo test -p frostsnap_nostr` green.
- Manual (user): create flow feels like local recovery's
  name→threshold pages; lobby reads status-first and matches the
  keygen lobby's card language; contributing flows through the
  footer primary, then the own-card `+`.
