# remote_recovery_nonces
# Remote recovery: verify nonce coverage, pin it, and add a scoped persist-time top-up

Counterpart to `remote_keygen_nonces`. Investigation says remote
recovery — unlike remote keygen — already gathers nonces at share
load time, because the lobby's "Load key share" reuses the local
recovery flow (`RecoveryFlowWithDiscovery` with
`RecoveryContext.remoteLobby()`), and that flow interposes
`generatingNonces` on both device paths BEFORE the share reaches
the lobby:

- device-with-share: "Contribute key share" → `confirmCandidate` →
  `generatingNonces` → completion (`recovery_flow.dart:200`);
- blank device + physical backup: `submitDeviceName` checks
  `someNoncesRequested()` → `generatingNonces` → backup entry
  (`recovery_flow.dart:246`).

So this plan is verification and hardening, not a hole-plug. If the
manual acceptance run below FAILS to sign, stop and report — that
means a different bug (first suspects: consolidation of
physical-backup devices, or streams consumed between load and
finish), and the plan should be re-scoped around what's found.

## Task 1 — pin the invariant

A regression test that a share cannot complete the remote-lobby
flow (`_completionResult` = `RemoteShareResultDeviceShare` /
`RemoteShareResultPhysicalBackup`) without the nonce stage having
run (or having confirmed nothing was needed). Pin it at whatever
seam is testable without a live coordinator — e.g. drive the
flow-controller's stage transitions with injected nonce
request/replenish closures (the seam style
`remote_keygen_nonces` introduces), asserting
`GeneratingNoncesStage` precedes completion in `remoteLobby`
context on both paths. If the controller resists injection,
extract the stage-decision into a testable unit rather than
skipping coverage.

## Task 2 — persist-time top-up, correctly scoped

Local recovery has a backstop: `onWalletRecovered`
(`wallet.dart:137-164`) sweeps the recovered access structure and
shows `NonceReplenishDialog` if anything is missing. Remote
recovery's `_persist()` (`remote_recovery_page.dart`) has no
equivalent. Add one — it covers streams consumed between load time
and lobby finish (a lobby can sit open for days while the same
device signs in other wallets).

CRITICAL SCOPING — do not copy the local backstop verbatim:
`accessStructure.devices()` in a recovered REMOTE wallet includes
other participants' devices; `replenishNonces` scoped to those
stalls forever waiting for hardware that never connects here.
Filter to devices that are both in the recovered access structure
AND locally present (currently connected, per the app's device
list). Empty filtered set or nothing requested → no dialog, no
flash. Devices unplugged at persist time are fine — they were
replenished at load time (Task 1's invariant).

## Non-goals

- No changes to the local recovery flow's stage machine beyond a
  test seam.
- No replenish-on-connect resurrect (`coordinator.rs` ~236).

## Tests

- Task 1's regression test(s), both share paths.
- Top-up filter: unit-test the device-set computation (AS ∩
  locally-present) — it must exclude a device id that is in the
  access structure but not locally present.
- Existing suites stay green.

## Acceptance

- `flutter analyze` + Dart suites + `cargo test` green.
- Manual (dual instance): 2-participant remote recovery ("I don't
  know" threshold is fine); after the wallet lands, each
  participant's signing device picker shows their own device
  enabled (no "no nonces remaining") and a nostr signing session
  completes end to end.
