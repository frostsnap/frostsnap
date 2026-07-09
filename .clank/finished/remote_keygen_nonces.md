# remote_keygen_nonces
# Remote keygen gathers signing nonces ("Preparing devices") at device confirmation

Remote keygen never replenishes nonce streams, so a wallet created
over nostr cannot sign: the first sign offer dies in `offer_to_sign`
(`frostsnap_core/src/coordinator/remote_signing.rs`) when
`nonce_cache.new_signing_session` returns
`NotEnoughNoncesForDevice`, and the signing device picker greys the
device out with "no nonces remaining"
(`device_selector.dart:26-32`).

Local keygen interposes a nonce step between device selection and
threshold (`WalletCreateStep { name, devices, nonceReplenish,
threshold }` in `wallet_create.dart`) — proof that nonce streams are
device-scoped and key-independent, so they can be gathered before
the key exists. The remote lobby has no equivalent anywhere: the
add-devices dialog (`_DeviceSetupDialog` → `LobbyAndKeygenController
.markReady`, `org_keygen_page.dart`) only posts device
registrations to the lobby, the `RemoteKeyGen` ceremony is pure
DKG, and finalize enrolls shares with zero streams on record.

## Fix: "Preparing devices" before markReady

In the add-devices dialog's submit sequence, before
`ctrl.markReady(devices)`:

1. `coord.createNonceRequest(devices: <the registered ids>)`.
2. If `someNoncesRequested()`: swap the dialog body from
   `DeviceSetupView` to the existing `NonceReplenishIndicator`
   (`nonce_replenish.dart`), title "Preparing devices", stream =
   `coord.replenishNonces(...).toBehaviorSubject()` — the same UI
   local keygen shows. On `NonceReplenishCompleted` → proceed to
   `markReady` and pop. On Aborted/Failed → back to the device
   list with the error in the existing `_submitError` banner;
   `markReady` must NOT have been posted. Back/cancel during the
   step → `coord.cancelProtocol()` and return to the device list.
3. If nothing is requested (e.g. re-confirming an edited set whose
   devices were already prepared) → `markReady` directly, no
   indicator flash.

Why this placement: the devices are guaranteed plugged in (the user
just named them in this dialog); "ready" in the lobby then means
genuinely able to sign later; one seam covers leader and joiners
alike (everyone registers devices through this dialog); and it is
idempotent on device-set edits since `createNonceRequest` only asks
for missing streams. No Rust changes — `createNonceRequest`,
`replenishNonces`, `cancelProtocol` all exist.

## Non-goals

- No post-finalize backstop over the access structure: the local
  pattern (`wallet.dart:144`, `accessStructure.devices()`) must not
  be copied here — in a remote wallet that set includes OTHER
  participants' devices and a nonce request scoped to them stalls
  waiting for hardware that never connects to this app.
- Do not resurrect the commented-out replenish-on-connect in
  `coordinator.rs` (~line 236) — separate design decision.

## Tests

Give the submit sequencing an injectable seam (closures for the
nonce request/replenish defaulting to `coord.*`, in the dialog or
its controller) so widget tests can drive it without a live
coordinator — `NonceReplenishState` is a plain data class and
`NonceReplenishIndicator` takes a plain `ValueStream`, so tests can
feed synthetic states (dylib init as in
`recovery_create_page_test.dart` for the sync `isFinished()` call):

- nonces needed → indicator shown; stream completes → `markReady`
  called once, dialog pops.
- stream aborts → error banner, back on the device list,
  `markReady` never called.
- nothing requested → `markReady` immediately, indicator never
  shown.

## Acceptance

- `flutter analyze` + Dart suites green.
- Manual (dual instance): run a 2-participant remote keygen; after
  the wallet lands, both participants' signing device pickers show
  their device enabled (no "no nonces remaining") and a nostr
  signing session completes end to end.
