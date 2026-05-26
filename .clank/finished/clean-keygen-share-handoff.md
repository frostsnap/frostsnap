# Clean keygen→channel share index handoff

## Problem

The Dart post-keygen code computes participant share indices by
flattening `ResolvedKeygen.participants` and using list position.
Share indices should come from the coordinator (the authority that
assigns them during keygen) — not reconstructed in Dart.

## Fix

### 1. `SendFinalizeKeygen` carries the full device→share map

```rust
pub struct SendFinalizeKeygen {
    pub local_devices: Vec<DeviceId>,     // was `devices`
    pub access_structure_ref: AccessStructureRef,
    pub keygen_id: KeygenId,
    pub device_to_share_index: HashMap<DeviceId, ShareIndex>,  // NEW: all devices
}
```

- Rename `devices` → `local_devices` (clarifies: these are the
  USB finalize targets, not the full participant set)
- Add `device_to_share_index` for ALL devices (not just local).
  Both `finalize_keygen` (local) and `finalize_remote_keygen`
  already compute this — they just discard non-local entries
  before returning. Keep the full map instead.
- `IntoIterator` uses `local_devices` for the USB `ToDevice`
  message (unchanged behavior).

### 2. App layer cross-references with resolved participants

In `keygen_run.rs`, `LoopContext::finalize`:
- Extract `finalized.device_to_share_index` BEFORE passing
  `finalized` to `send_from_core` (which consumes it)
- Cross-reference with `resolved.participants` (pubkey→devices,
  stored on `LoopContext`) to build `Vec<ChannelParticipant>`
- Return `RemoteKeygenResult` from `finalize`

### 3. `confirmMatch` returns `RemoteKeygenResult`

```rust
pub struct RemoteKeygenResult {
    pub access_structure_ref: AccessStructureRef,
    pub participants: Vec<ChannelParticipant>,
}
```

`LoopContext::finalize` builds this. The `ConfirmMatch` command
handler sends it back via the oneshot reply channel. Dart's
`confirmMatch` async call resolves with it.

### 4. Dart: `_confirmAndFinalize` drives the transition

Today `_confirmAndFinalize` ignores `confirmMatch`'s return value,
and the `KeyGenState.finished` listener drives the pop/channel
setup via `_dismissOverlayThenPop`. This creates two parallel
paths for the same event.

Change: `_confirmAndFinalize` uses the `RemoteKeygenResult`
directly. A `_remoteFinalizeInFlight` guard is set BEFORE calling
`confirmMatch` so the `finished` listener cannot race:

```dart
bool _remoteFinalizeInFlight = false;

Future<void> _confirmAndFinalize() async {
  _remoteFinalizeInFlight = true;
  final result = await _ctrl.confirmMatch(encryptionKey: encKey);
  // result has access_structure_ref + participants
  await _dismissOverlayThenPop(result);
}
```

The `KeyGenState.finished` listener checks the guard:

```dart
if (kgState.finished != null && !_popped && !_remoteFinalizeInFlight) {
  // Only fires for local keygen (no confirmMatch in that flow)
  ...
}
```

This prevents the race: `keygen_finalized` fires on the broadcast
BEFORE `confirmMatch` replies to Dart, but the guard is already
true so the listener skips. `_confirmAndFinalize` handles the
transition with the full `RemoteKeygenResult`.

## What stays the same

- `KeyGenState` struct untouched (no channel concepts)
- `NewShare` mutations still only emitted for local devices
- Core doesn't know about nostr pubkeys
- Local keygen path unchanged (still uses `KeyGenState.finished`
  listener — no `confirmMatch` in that flow)
- `IntoIterator` for `SendFinalizeKeygen` sends USB finalize to
  `local_devices` only (same behavior)

## What changes

- `SendFinalizeKeygen`: `devices` → `local_devices`, add
  `device_to_share_index: HashMap<DeviceId, ShareIndex>`
- `finalize_keygen` + `finalize_remote_keygen` populate full map
- `LoopContext::finalize` returns `RemoteKeygenResult`
- `confirmMatch` return type: `AccessStructureRef` →
  `RemoteKeygenResult`
- Dart `_confirmAndFinalize` uses return value, drives transition
- Dart removes `devicesInOrder` / `indexOf` computation

## Verification

- `cargo check --workspace` clean
- `flutter analyze lib` clean
- `just gen` after API change
- Dart no longer contains any share index computation
- Remote keygen → lands in remote wallet (not local)
- `KeyGenState.finished` listener doesn't double-fire
