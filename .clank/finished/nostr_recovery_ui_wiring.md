# nostr_recovery_ui_wiring
# Wire remote recovery into the app

`nostr_recovery_transport` + `nostr_recovery_lobby_ui` landed the
Rust surface and the two Dart pages (`RemoteRecoveryEntryPage`,
`RemoteRecoveryLobbyPage`), but grep shows the entry page has
**zero** references outside `lib/recovery/` — the flow is
unreachable. This plan finishes the wiring: entry point, deep
links, post-recovery navigation, and the one FRB-wrapper gap that
prevents the local device from consolidating after finalize.

Depends on: nothing new; consumes the surface delivered by
[[nostr_recovery_transport]] and [[nostr_recovery_lobby_ui]].

## Invariants

1. **All the primitives are already in `frostsnap_core`.** The core
   `finalize_remote_recovery` creates the key and queues
   `PendingConsolidation` mutations for each local device;
   `consolidate_pending_physical_backups(device_id, encryption_key)`
   produces the USB `Consolidate` messages; `exit_recovery_mode` on
   the app-side `Coordinator` (`frostsnapp/rust/src/coordinator.rs`)
   is the loop-until-`FinishedConsolidation` primitive already used
   by `finish_restoring`. This plan does NOT add core APIs — it
   drives the ones that exist.

2. **Symmetry with local restoration for post-finalize
   consolidation.** `finish_restoring` at
   `frostsnapp/rust/src/coordinator.rs:864` loops over the
   restoration's `needs_to_consolidate()` and calls
   `exit_recovery_mode(device_id, encryption_key)` for each. The
   remote wrapper `finalize_remote_recovery_from_transport` (same
   file, line 1275) must do the same, using the `my_local_devices`
   set it already receives.

3. **Nav parity with local restoration.** Recovery landing = the
   same landing local restoration uses:
   `homeCtx.openNewlyCreatedWallet(asRef.keyId)` after a
   `showUnplugDevicesDialog(context)` prompt. The lobby page pops
   the `AccessStructureRef` up the navigator; the caller
   (`wallet_add.dart`) handles the two calls once, matching
   `showWalletCreateDialog` exactly.

4. **One "Restore" surface in the wallet picker.** The recovery
   entry is a sibling card under the existing "Restore wallet"
   heading in `WalletAddColumn` (`wallet_add.dart`), not a new
   top-level section. This mirrors how a user thinks about it:
   "restore a wallet from a physical backup — alone or with others."

5. **Deep links route by prefix, not by page.** `frostsnap://channel/`
   still goes to `JoinFromLinkPage` (wallet-create join).
   `frostsnap://recovery/` goes to the recovery join flow. Routing
   lives in one place — `_handleDeepLink` in `main.dart` — so both
   prefixes are handled at the same layer. `JoinFromLinkPage` does
   not learn about recovery links, and vice versa.

6. **Network + name are picked by the leader before the lobby
   exists.** The existing `_CreateLobbyDialog` in
   `remote_recovery_entry_page.dart` hard-codes
   `BitcoinNetwork.bitcoin`. Replace the hard-code with a
   `BitcoinNetworkChooser` (reuse the widget already used by
   `wallet_create.dart` / restoration). Do not introduce a new
   chooser or a new form abstraction — the dialog gains one field.

## Files touched

- `frostsnapp/rust/src/coordinator.rs` —
  `finalize_remote_recovery_from_transport` gains the post-finalize
  consolidation loop (see §Consolidation).

- `frostsnapp/lib/recovery/remote_recovery_lobby_page.dart` —
  after `_persist()` sets `_recoveredRef`, schedule a
  `Navigator.pop(context, _recoveredRef)` so callers can chain onto
  `openNewlyCreatedWallet`. Add a "Continue" button on the finished
  banner as a fallback for the joiner case (leader auto-pops after
  the persist banner shows briefly).

- `frostsnapp/lib/recovery/remote_recovery_entry_page.dart` —
  `_CreateLobbyDialog` gains a `BitcoinNetworkChooser`; the result
  carries `BitcoinNetwork`. `_connect(...)` reads that and passes
  `keyPurposeBitcoin(network: ...)` instead of the mainnet
  hard-code. `_connect` returns the `AccessStructureRef` popped by
  the lobby page (not `void`), and `RemoteRecoveryEntryPage`'s own
  Scaffold uses `Navigator.pop(context, asRef)` on success so the
  outer `MaybeFullscreenDialog` unwinds with the ref.

- `frostsnapp/lib/wallet_add.dart` —
  - Add `AddType.remoteRecoverWallet`.
  - Add a fourth `buildCard` under the "Restore wallet" heading
    (label: "Restore wallet with participants", subtitle: "Combine
    your share with others over nostr").
  - Add `WalletAddColumn.showRemoteRecoveryDialog(context)`,
    modelled on `showWalletCreateDialog`: it calls
    `MaybeFullscreenDialog.show<AccessStructureRef>(child:
    RemoteRecoveryEntryPage(...))`, awaits, then
    `showUnplugDevicesDialog` + `openNewlyCreatedWallet(asRef.keyId)`.
    An optional `initialJoinLink` argument (for deep links) is
    forwarded to the entry page.
  - Extend `makeOnPressed` with the new case.

- `frostsnapp/lib/main.dart` — `_handleDeepLink` dispatches on
  `uri.host`: `"channel"` → `showJoinFromLinkDialog(initialLink:
  ...)` (unchanged); `"recovery"` → `showRemoteRecoveryDialog(
  initialJoinLink: ...)`. No changes to `AppLinks` setup.

## Consolidation — the FRB-wrapper gap

`FrostCoordinator::finalize_remote_recovery` (in `frostsnap_core`)
persists the key and queues `PendingConsolidation` for each local
device, but produces no USB messages by itself. The local flow's
`Coordinator::finish_restoring` (in
`frostsnapp/rust/src/coordinator.rs`) then drives the USB round-
trip by calling `exit_recovery_mode` for each device that needs
consolidation. The remote wrapper
`finalize_remote_recovery_from_transport` does not.

Change:

```rust
pub(crate) fn finalize_remote_recovery_from_transport(
    &self,
    ras: &...RecoveringAccessStructure,
    key_name: String,
    purpose: KeyPurpose,
    my_local_devices: &BTreeSet<DeviceId>,
    encryption_key: SymmetricKey,
    rng: &mut impl RngCore,
) -> Result<AccessStructureRef> {
    let asr = { /* existing staged_mutate block */ };

    for device_id in my_local_devices.iter().copied() {
        // no-op if the device isn't connected or has nothing pending
        self.exit_recovery_mode(device_id, encryption_key);
    }

    self.emit_key_state();
    Ok(asr)
}
```

`exit_recovery_mode` already:
- reads `consolidate_pending_physical_backups(device_id, ..)` from
  the coordinator (which now contains the freshly-queued
  `PendingConsolidation`),
- sends the resulting `Consolidate` USB message,
- blocks for `FinishedConsolidation`,
- flips the device to `DeviceMode::Ready`.

No new arguments to `persistRecovered` — `my_local_devices` is
already derived in `RemoteRecoveryLobbyHandle::persist_recovered`
from the state broadcast (`frostsnap_nostr::recovery::my_local_devices`).

Rust unit-test extension: extend `frostsnap_core/tests/remote_recovery.rs`
to assert that `has_backups_that_need_to_be_consolidated(local_device)`
flips false after `consolidate_pending_physical_backups` runs and the
`Consolidate` message is folded (the existing test already runs the
consolidate step at lines 125-129 but only checks share tracking; add
an `assert!(!coord.has_backups_that_need_to_be_consolidated(local_device))`
at the end).

FRB-side smoke test in `frostsnapp/rust/tests/` (companion to the
existing `recovery_live`): drive
`Coordinator::finalize_remote_recovery_from_transport` through a
mock device-message loop and assert the device ends up in
`DeviceMode::Ready`. If that harness doesn't exist for this crate,
skip and let `recovery_live` cover it end-to-end (call out in the
plan's acceptance).

## Post-recovery navigation

`RemoteRecoveryLobbyPage._persist()` currently sets `_recoveredRef`
and stops. Change:

- Every participant runs `persistRecovered` on `finished` — no
  leader-vs-joiner branch. Persistence is a per-participant call:
  `RemoteRecoveryLobbyHandle::persist_recovered`
  (`frostsnapp/rust/src/api/nostr/remote_recovery.rs:328-342`)
  derives `my_local_devices` from that participant's own SharePosts
  via `frostsnap_nostr::recovery::my_local_devices`
  (`frostsnap_nostr/src/recovery/lobby.rs:532-540`). A joiner who
  posted a physical share needs the same finalize + consolidation
  round as the leader; a leader who posted none gets an empty
  `my_local_devices` and the consolidation loop is a no-op — same
  code path, different derived set.
- After `_recoveredRef` is set, call
  `WidgetsBinding.instance.addPostFrameCallback(...)` to pop the
  route with the `AccessStructureRef`. The finished banner flashes
  once — sufficient signal that persist landed.
- The pop value is always the `AccessStructureRef` (never null).
  Downstream, `openNewlyCreatedWallet(asRef.keyId)` opens the newly
  persisted wallet for every participant, whether they consolidated
  a local device or not — the wallet is theirs to view either way.

`WalletAddColumn.showRemoteRecoveryDialog`:

```dart
static Future<void> showRemoteRecoveryDialog(
  BuildContext context, {
  String? initialJoinLink,
}) async {
  final homeCtx = HomeContext.of(context)!;
  final nostrClient = await NostrContext.of(context).nostrClient;
  if (!context.mounted) return;
  final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
    context: context,
    barrierDismissible: false,
    child: RemoteRecoveryEntryPage(
      coord: coord,
      nostrClient: nostrClient,
      initialJoinLink: initialJoinLink,
    ),
  );
  if (asRef == null || !context.mounted) return;
  await showUnplugDevicesDialog(context);
  if (!context.mounted) return;
  homeCtx.openNewlyCreatedWallet(asRef.keyId);
}
```

`RemoteRecoveryEntryPage` gains an `initialJoinLink` param. If
non-null, on mount it auto-triggers `_joinLobby()` with the link
prefilled — same pattern `JoinFromLinkPage` uses for its
`initialLink`.

## Deliberately NOT done

- No leader-disconnect heartbeat / "leader left" UI.
- No QR-scan for the recovery link. Users paste (or arrive via
  deep-link).
- No signing-channel hop after persist — the recovered wallet
  appears in the wallet list; signing is a separate ceremony.
- No local pre-post share-image validation — the transport already
  surfaces `FinishVerificationFailed` via `awaitFinished()`, and
  the lobby page already renders that error state.
- No physical-share flow for `AppKey` purpose. Load-share continues
  to hard-code `DeviceKind.frostsnap`.

## Acceptance

- `flutter test test/recovery_lobby_view_test.dart` still green
  (existing tests unaffected; navigation is driven by the hosting
  page, not `RecoveryLobbyView`).
- `cargo test -p frostsnap_core --test remote_recovery` green with
  the extended consolidation assertion.
- Widget test: add `test/recovery_entry_page_test.dart` that pumps
  `RemoteRecoveryEntryPage` with a stub `NostrClient`, taps
  "Create", enters a name + network, and asserts
  `createRemoteRecoveryLobby` was called with the expected
  `KeyPurpose` matching the chosen network. (Stub is a thin
  `NostrClient` mock — no live handle.)
- Manual (deferred to user): with two sim/hw participants, run
  Create → paste link on joiner → leader Recover → observe both
  clients land in the wallet list; local device signs a regtest
  send. This is the `recovery_live`-style walkthrough, kept out of
  CI.

## Order of work

1. Fix `finalize_remote_recovery_from_transport` (Rust) + extend
   the `remote_recovery` test.
2. Wire post-recovery pop in `RemoteRecoveryLobbyPage` +
   `RemoteRecoveryEntryPage`.
3. Extend `_CreateLobbyDialog` with `BitcoinNetworkChooser`.
4. Add `AddType.remoteRecoverWallet` +
   `showRemoteRecoveryDialog` + card in `WalletAddColumn`.
5. Extend `_handleDeepLink` in `main.dart`.
6. Add the entry-page widget test.
