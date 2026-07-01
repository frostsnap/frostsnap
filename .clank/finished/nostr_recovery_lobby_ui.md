# nostr_recovery_lobby_ui
# Recovery lobby UI

Dart-side pages for the remote recovery flow. Shape is a merger of
two existing UX patterns:

- **Keygen lobby** (`frostsnapp/lib/nostr_chat/keygen_lobby_page.dart`
  and friends): participant list, invite link, leader controls
  (Start / Cancel), joiner controls (Register / Leave), waiting
  states.
- **Local recovery** (`frostsnapp/lib/recovery/` — enter backup,
  fuzzy-progress display, share list, "Recover" button when a
  valid subset is available).

The user should feel that remote recovery is the same lobby ceremony
they know from keygen, but the "commit devices" step is replaced by
"enter or plug in your share", and the final action is "Recover"
instead of "Keygen".

Depends on the transport plan
([[nostr_recovery_transport]]) landing first — this plan consumes
its FRB surface (`RemoteRecoveryLobbyHandle`,
`RecoveryLobbyState`, `RecoveredKey`, `FinishedRecovery`,
`NostrClient::create_remote_recovery_lobby` /
`join_remote_recovery_lobby`).

## Design invariants

1. **Single source of truth is `RecoveryLobbyStateBcast`.** The UI
   is a fold of the broadcast — no local view state that duplicates
   what the transport already tracks. Matches the pattern in
   `remote_keygen`'s `LobbyStateBcast` consumer.
2. **`handle.subState().watch()` — not `.events()`.** The recovery
   handle is a value-shaped subscription: `subState()` returns a
   `RecoveryLobbyStateBcast` whose `.watch()` yields a
   `Stream<RecoveryLobbyState>`. The chat-side
   `handle.events()` / `handle.start()` / `handle.close()` triad
   from `per_consumer_channel_runner` does NOT apply here —
   `create_remote_recovery_lobby` / `join_remote_recovery_lobby`
   already spawn the runner and return only AFTER the first
   `StateChanged` is seeded on the bridge, so there is no lazy
   `start()` to await.
3. **Dispose = cancel the stream subscription; drop the handle.**
   The FRB-opaque handle has no `close()` method; letting the
   `RustOpaqueInterface` go out of scope + cancelling the
   `StreamSubscription` is the whole teardown. If we later grow a
   `close()` on the handle, revisit this invariant.
4. **Fold-derived progress; no `RecoveringAccessStructure`
   surface.** `RecoveredKey` at the FRB layer carries only
   `accessStructureRef` + `winningShareRefs`; there is no
   `.effective_threshold()` / `.compatibility()` on the wire type.
   Progress display sources from what IS available:
   `state.metadata.thresholdHint` (the leader's stated hint,
   optional), `state.shares.length` (total posted), and
   `state.currentRecovery != null` (fuzzy recovery has landed).
5. **`SharePost` (not `RecoverShare`) on the wire.** The Dart FFI
   surface is `handle.postShare({required SharePost post})`.
   `SharePost` fields: `deviceId`, `deviceName`, `deviceKind`,
   `shareImage`, `needsConsolidation`. Backup-entry produces a
   `RecoverShare` locally; the entry flow ends by unpacking that
   into a `SharePost` (see §"Share entry → SharePost conversion"
   below) and calling `postShare`.
6. **Local-device derivation lives in Rust.** `persistRecovered`
   internally derives `my_local_devices` from
   `state.participants[me].postedShares` → `ObservedShare.post
   .deviceId`, so the Dart page MUST NOT track its own local-devices
   set. The Rust helper `frostsnap_nostr::recovery::my_local_devices`
   is the source of truth (unit-tested in the transport plan).
7. **Post-recovery hop.** On `Finished`, the page calls
   `handle.persistRecovered(coord: coord, encryptionKey:
   encryptionKey)` to get the `AccessStructureRef`, then navigates
   to the standard post-restoration flow (existing local-recovery
   finish page) OR opens a signing channel keyed by the recovered
   ASref (deferred to a follow-up plan; this UI just navigates to
   a placeholder / success page).

## Frontend-design pass

Invoke the `frontend-design` skill early in this plan's execution
to produce mockups for the three primary states:

1. **Leader creating a channel**: wallet-name / purpose entry,
   optional threshold hint, "Create recovery channel" button →
   waiting state with invite-link display + participant list.
2. **Joiner entering the lobby**: invite-link paste / QR scan,
   name display for wallet being recovered, participant list.
3. **Share entry + progress**: enter-backup dialog (matches
   local recovery), share list with per-participant grouping,
   overall reconstruction progress (using
   `RecoveryLobbyState.metadata.thresholdHint` +
   `state.shares.length` +
   `state.currentRecovery != null`), and — for the leader only
   when `state.currentRecovery != null` — a "Recover" button.
4. **Post-Finish**: non-leaders see "Recovery in progress —
   confirming with your device" then success; leader sees the
   same success page.

The frontend-design skill should NOT converge on the same
aesthetic as the keygen lobby's — this is a different ceremony
and deserves visual differentiation (perhaps a warmer / more
reassuring palette given the emotional weight of "recovering a
wallet I might have lost"). Design choices should be justified
inline in the plan post-frontend-design pass.

## Page structure

`frostsnapp/lib/recovery/remote_recovery_lobby_page.dart` — top-level
`StatefulWidget` that owns an ALREADY-STARTED `RemoteRecoveryLobbyHandle`.
The entry-point functions (`create_remote_recovery_lobby` /
`join_remote_recovery_lobby`) return a handle whose bridge broadcast
is already seeded with the first `RecoveryLobbyState` — this page
does not construct it.

State:

- `_handle: RemoteRecoveryLobbyHandle` — passed via constructor
  (created by the entry page — see next section).
- `_subscription: StreamSubscription<RecoveryLobbyState>?` — from
  `handle.subState().watch().listen(_onState)`.
- `_current: RecoveryLobbyState?` — latest fold snapshot for
  `build`. Non-null after the first `_onState` call.
- `_persisting: bool` / `_recoveredRef: AccessStructureRef?` /
  `_error: String?` — persist-recovered UI state.

Deliberately NOT tracked here:

- Local-device set. `persistRecovered` derives it in Rust; if the
  Dart layer duplicates the derivation it will drift.
- `RecoveringAccessStructure`. Not FRB-exposed; use
  `RecoveredKey.winningShareRefs` + `state.metadata.thresholdHint`.

Lifecycle:

- `initState` → `_sub = handle.subState().watch().listen(_onState)`.
- On `_onState`: `setState(_current = state)`. If
  `state.finished != null && _recoveredRef == null && !_persisting`:
  fire `_persist()`.
- `dispose` → `_sub?.cancel()`. Handle disposal is the caller's
  responsibility (the entry page navigates HERE with the handle
  and pops the whole route when done — the RustOpaqueInterface
  goes out of scope at that point).

## Interactions with existing widgets

Reuse (don't fork):

- **Enter-backup / device-load flow** from `frostsnapp/lib/restoration/`
  (`recovery_flow.dart` orchestrates `tellDeviceToEnterPhysicalBackup`
  and folds the `EnterPhysicalBackupState` stream). This plan does
  NOT re-implement backup entry — it wraps the same
  `EnterPhysicalBackupState` outcome and converts to a `SharePost`
  (see next section).
- **Share compatibility badge** — local recovery already renders
  `ShareCompatibility::{Compatible, Incompatible, Uncertain}` per
  share. NOTE: this badge is derived from the local
  `RecoveringAccessStructure`, which the remote-recovery fold does
  NOT expose. We can either (a) drop the per-share badge for the
  remote UI, or (b) rebuild an RAS Dart-side from
  `state.shares` (all posts) to compute compatibility. Pick (a)
  for the first cut; revisit if usability demands it.
- **Threshold progress indicator** — sourced from
  `state.metadata.thresholdHint` (leader-stated hint) +
  `state.shares.length`. Displayed as "N posted (target K)" when
  the hint is present, or "N posted" when absent.

New:

- **Participant row** — avatar + display name (from
  `RecoveryParticipantInfo.profile` if present, else
  `pubkey.toNpub()`) + a chip showing `postedShares.length` +
  "left" badge if `info.left`.
- **Invite-link display + copy button** — reads
  `handle.inviteLink()`.
- **Leader-only "Recover" button** — enabled iff
  `state.currentRecovery != null && state.finished == null`.
  Wired to `handle.finish(shareRefs:
  state.currentRecovery!.winningShareRefs)`.

## Share entry → SharePost conversion

The existing `restoration/recovery_flow.dart` produces a fully-
populated `RecoverShare` (with `held_share2` containing
`shareImage`, `needsConsolidation`, etc.) as the backup-entry
outcome. To turn that into a `SharePost` we drop the "trusted"
fields (`accessStructureRef`, `threshold`, `keyName`, `purpose`)
that the wire schema does not carry — matching the Rust-side
`SharePost.to_recover_share()` seatbelt in reverse:

```dart
SharePost sharePostFromRecoverShare(RecoverShare share, String
    deviceName, DeviceKind deviceKind) => SharePost(
      deviceId: share.heldBy,
      deviceName: deviceName,          // from `DeviceNames`
      deviceKind: deviceKind,          // from device inventory
      shareImage: share.heldShare.shareImage,
      needsConsolidation: share.heldShare.needsConsolidation,
    );
```

If `EnterPhysicalBackupState` exposes only intermediate stages, the
conversion happens at the "backup entered successfully" leaf —
same integration point local recovery uses to move on.

## `Finished` and persist

The `_onState` listener triggers `handle.persistRecovered(...)`
exactly once when the fold flips `state.finished != null`. Guard:

- `_recoveredRef == null` (haven't persisted yet), AND
- `!_persisting` (don't double-fire while an in-flight persist is
  outstanding), AND
- `_error == null` (a prior persist error should surface the retry
  UI, not silently retry).

`persistRecovered` returns the `AccessStructureRef` on success;
the page stashes it in `_recoveredRef` and renders a "recovery
complete" banner. Navigation to the standard post-restoration
flow is a follow-up (see §"Does NOT do" below).

## Error / edge states

- **Leader disconnect during a Finish flight**: participants
  observe transport shutdown but no Finished. Show "Leader left
  the lobby — recovery cancelled" and pop the route (handle
  disposal cascades via the state subscription cancel).
- **`FinishVerificationFailed`**: the transport fires this via
  `awaitFinished()` returning an error; the fold does NOT surface
  a boolean on the state broadcast, so the page has to watch it
  via a side-channel. Simplest: on `initState`, spawn
  `handle.awaitFinished()` in a fire-and-forget future; on error
  (any error from that future), setState an error banner and
  disable the Recover button. Show "The leader's Finish message
  doesn't match — this is a protocol bug or the leader is
  malicious. Aborting." Log loudly. No explicit close call —
  cancel the subscription and pop the route.
- **Duplicate share post from the same participant**: the fold
  keeps all of them (per transport plan invariant #4 —
  "multiple shares per participant fine"). UI groups them under
  the participant in the list.
- **User closes the app mid-flow**: dispose cancels the state
  subscription; the RustOpaqueInterface handle falls out of scope
  and the runner task teardown chain runs. On relaunch, the
  invite link (if still known to the user) can be pasted to
  rejoin. No auto-restore.

## What this plan deliberately does NOT do

- **Signing channel hop.** On successful recovery + persist, the
  page navigates to a placeholder "recovery complete" screen. The
  real "open the recovered wallet in the signing UI" hop is a
  follow-up plan (spelled out in the finalized `nostr_recovery`
  plan's §Follow-up).
- **Invite-link QR scanning / camera integration.** Paste-only
  for the first cut; QR is a follow-up.
- **Deep-link handling** (`frostsnap://recover/...`). Follow-up.
- **Multi-wallet parallel recoveries.** One handle per page
  instance; no cross-recovery state.

## Tests

- **Widget test** in `frostsnapp/test/` covering the state
  transitions: empty lobby → participants join → shares
  accumulate → `state.currentRecovery != null` → leader clicks
  Recover → Finished.
- **Manual acceptance** via `just run-dual` (or triple with a
  third participant) end-to-end run.

## Verification

- `flutter analyze lib` clean.
- Widget test passes.
- Manual: two participants can complete a full recovery flow,
  land on the success screen, and the recovered wallet is
  visible in the wallet list.
