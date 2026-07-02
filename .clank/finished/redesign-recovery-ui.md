# redesign-recovery-ui
# Make the remote-recovery UI speak the app's design language

The remote recovery UI shipped by [[nostr_recovery_lobby_ui]] /
[[nostr_recovery_ui_wiring]] is functional but inconsistent with the
rest of the app and visually low-quality:

- The lobby is a plain `Scaffold` pushed as a `MaterialPageRoute`,
  while every comparable ceremony (keygen lobby, local recovery,
  wallet create) runs as `MultiStepDialogScaffold` steps inside a
  `MaybeFullscreenDialog`.
- "Load share" opens a bottom-sheet device chooser + a bare
  `AlertDialog` spinner. The app's established pattern for "get a
  device involved" is the glowy plug-in prompt
  (`AnimatedGradientPrompt` driven by `coord.waitForSingleDevice()`
  in `restoration/device_discovery.dart`) that reacts when a device
  is plugged in and branches blank-device vs device-with-share.
- Functional gap the redesign closes: the current load-share flow
  ONLY supports entering a physical backup. A device that already
  holds a share for the wallet (`WaitForSingleDeviceState_DeviceWithShare`
  → `RecoverShare`) cannot be posted to the lobby at all — even
  though local recovery handles exactly this branch.

The `frontend-design` skill is engaged for this plan — the design
direction below comes from it, and Task 3 runs its composition/copy
pass on the final layout.

## Design direction (frontend-design pass)

- **Purpose**: a multi-party recovery ceremony. The user's emotional
  state is "I lost my wallet and I'm getting it back with friends" —
  the UI must feel calm, guided, and identical in cadence to the
  keygen ceremony they've already done. Familiarity IS the design.
- **Tone**: the app's established ceremony language — dark Material 3,
  `MultiStepDialogScaffold` steps with forward/back animation,
  filled cards on `surfaceContainerHigh`, one glowing
  `AnimatedGradientPrompt` as the single point of "do this now"
  attention. No new palette, no new typography; the deliverable is
  coherence, not novelty.
- **Composition**: lobby step reads top-to-bottom as a narrative —
  wallet name (headline) → who's here (participant rows with
  avatars) → what we've collected (share progress) → the one glowing
  action (plug in a device / share the invite) → footer commitment
  buttons. Exactly one glowing element on screen at a time: the
  plug-in prompt when shares are needed, nothing once Recover is
  available (the primary button carries the emphasis then).
- **Reactivity**: plugging in a device visibly changes the screen
  (discovery card reacts, then flows into naming/backup entry) —
  never a static list with a modal bottom sheet.

## Coupling assessment (per the draft's "fix that first" question)

`DeviceDiscoveryWidget` + `WalletRecoveryFlow` are bound to local
recovery only through the `RecoveryContext` sealed class
(`restoration/state.dart`): `_validateShare` and `_getPromptText`
switch over its three variants, and `recovery_flow.dart`'s
`_onBackupEntered` + stage-transition helpers switch again. The
binding is a handful of exhaustive switches, not a structural
entanglement — adding a fourth variant is mechanical. **Verdict: fix
in this plan (Task 1), no separate plan needed.**

## Design invariants

1. **Same widgets, same flow.** The remote share-entry path reuses
   `RecoveryFlowWithDiscovery` — the exact widget local recovery
   uses — via a new `RecoveryContext.remoteLobby` variant. No
   parallel implementation of discovery, firmware-upgrade, device
   naming, or backup entry.
2. **Remote context ends by handing back a result, not by writing
   coordinator state.** Local contexts terminate in
   `tellDeviceToSavePhysicalBackup` / share enrollment. The
   `remoteLobby` context terminates by popping a result the lobby
   converts to a `SharePost`:
   - blank device → firmware-upgrade (if needed) → name device →
     nonces → enter backup → pop `PhysicalBackupPhase` + device
     name (`needsConsolidation: true`);
   - device with share → `candidateReady` confirm → pop the
     `RecoverShare` (`needsConsolidation:
     recoverShare.heldShare.needsConsolidation`).
   The wallet-name / network / threshold stages
   (`enterRestorationDetails`, `enterThreshold`) are skipped —
   lobby metadata already carries keyName/purpose/thresholdHint
   from the leader.
3. **No local share validation in the remote context.** Local
   contexts validate against a restoration/access structure via
   coord checks; the lobby has neither. The transport already
   dedupes and the fold rejects incompatible shares at Finish.
   `_validateShare` returns null for `remoteLobby`.
4. **Lobby page = MultiStepDialogScaffold steps in one
   MaybeFullscreenDialog,** mirroring `OrgKeygenPage` →
   `LobbyAndKeygenPage`: `showRemoteRecoveryDialog` shows a single
   multi-step page whose steps are create-form (leader only) →
   lobby → done. Joiners enter at the lobby step via
   `dispatchJoin`. The standalone `RemoteRecoveryCreatePage`
   Scaffold and the `MaterialPageRoute` push of
   `RemoteRecoveryLobbyPage` go away.
5. **Shared invite affordance.** The keygen lobby's invite tile +
   QR invite dialog (`_InviteTile` / `_showInviteDialog` with
   `PrettyQrView` in `org_keygen_page.dart`) are extracted to a
   shared widget and used by both lobbies. The recovery lobby's
   QR is what the join-side `QrStringScanner` (from
   [[join_via_link_unified]]) scans.
6. **Keep a pure state→UI view layer.** The `RecoveryLobbyView`
   extraction exists so `recovery_lobby_view_test.dart` can drive
   the UI without a live handle. The redesigned lobby step keeps
   that split; the 7 existing tests are adapted, not deleted.
7. **No FullscreenActionDialog.** Nothing in this ceremony requires
   confirming an action on the device screen; seed-word entry is
   already covered by the reused `enterBackup` stage.

## Tasks

1. **`RecoveryContext.remoteLobby` + result-popping terminals.**
   - Add the variant in `restoration/state.dart` (freezed) — carries
     nothing (metadata lives in the lobby).
   - Define a small sealed result type (e.g. `RemoteShareResult.
     physicalBackup(phase, deviceName)` / `.deviceShare(share)`).
   - Update the exhaustive switches: `DeviceDiscoveryWidget.
     _validateShare` (return null), `_getPromptText` ("Plug in a
     Frostsnap to load a share into the recovery."),
     `recovery_flow.dart` `_onBackupEntered` (pop result instead of
     save), initial-stage selection + transitions (skip
     `enterRestorationDetails`/`enterThreshold`; `candidateReady`
     confirm pops result instead of enrolling).
   - `flutter analyze` will enumerate every switch that needs the
     new arm — freezed makes misses compile errors, not runtime
     surprises.

2. **Extract shared invite tile + QR dialog** from
   `org_keygen_page.dart` into `lib/invite_widgets.dart` (tile +
   dialog with QR, copy button, link text). Keygen lobby keeps its
   behavior; recovery lobby adopts it.

3. **Rebuild the lobby as multi-step dialog.** Merge
   `RemoteRecoveryCreatePage` + `RemoteRecoveryLobbyPage` into one
   stepped page (shape of `LobbyAndKeygenPage`):
   - Steps: `createForm` (leader; the existing CreateLobbyDialog
     fields as a step, not an AlertDialog) → `lobby` → terminal
     banner/pop.
   - Lobby step: participant rows styled like keygen's
     `_participantRows` (NostrAvatar + name + posted-share chip),
     shared invite tile, share progress, glowy prompt slot for the
     load-share affordance, footer = Cancel lobby / Leave +
     primary Recover button (leader) — matching keygen's footer
     layout.
   - `showRemoteRecoveryDialog` + `dispatchJoin` updated to the
     merged page. Post-persist pop semantics (AccessStructureRef →
     unplug prompt → `openNewlyCreatedWallet`) unchanged.
   - Run the `frontend-design` skill pass on this step's layout.

4. **Replace `_loadShare`.** Delete `_DeviceChooserSheet`,
   `_AwaitBackupDialog`, `_BackupEntryController`. "Load share"
   pushes `RecoveryFlowWithDiscovery(recoveryContext:
   RecoveryContext.remoteLobby())` inside `MaybeFullscreenDialog`,
   awaits the `RemoteShareResult`, converts to `SharePost`
   (device-share path uses `heldShare.needsConsolidation`), posts
   via `handle.postShare`.

5. **Tests.**
   - Adapt `recovery_lobby_view_test.dart` to the redesigned pure
     view (same 7 behaviors).
   - Adapt `recovery_create_page_test.dart` to the create-form
     step (same validation + network assertions; `dispatchCreate`
     unchanged).
   - New unit test for the `RemoteShareResult` → `SharePost`
     conversion (needsConsolidation faithfulness for both arms).

## Deliberately NOT done

- No transport/FRB/Rust changes — `postShare`, `persistRecovered`,
  lobby state broadcast are untouched.
- No changes to local recovery behavior — the three existing
  contexts keep byte-identical flows; only new switch arms appear.
- No keygen-lobby visual changes beyond the invite-widget
  extraction.

## Acceptance

- `flutter analyze` clean; `dart format` clean on touched files.
- `flutter test test/recovery_lobby_view_test.dart
  test/recovery_create_page_test.dart test/join_link_dispatch_test.dart`
  green (adapted).
- Existing local-recovery flows still compile with exhaustive
  switches (analyzer proves the new arm everywhere).
- Manual walkthrough (user): create lobby → glowy plug-in prompt →
  blank device enters seed words → share appears in lobby;
  device-with-share plugs in → share posts directly; leader
  Recover → wallet opens. Same ceremony feel as keygen lobby.
