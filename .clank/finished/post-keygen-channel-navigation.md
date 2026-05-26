# Post-keygen navigation doesn't enter remote wallet

## Bug

After completing a remote keygen, the app navigates to the local
wallet shell instead of the remote (chat-first) shell.

## Root cause

`_dismissOverlayThenPop` in `org_keygen_page.dart` derives
`ChannelParticipant.shareIndices` from the local `AccessStructure`
via `getDeviceShortShareIndex`. After remote keygen, the local
access structure only has local devices (`finalize_remote_keygen`
emits `NewShare` only for local devices). For remote participants'
devices the lookup returns null → `StateError` → caught silently →
`setCoordinationUiEnabled` never runs → local wallet.

## Fix

### 1. Derive share indices from ResolvedKeygen, not the coordinator

`ResolvedKeygen.participants` carries all participants in
`StartKeygen` order. Share indices are positional (1-based position
in the flattened device list). Compute them directly from the
keygen result — no coordinator lookup needed.

### 2. Automatic retry — never fail

Channel setup retries automatically on failure. The user sees a
visible "Setting up signing channel..." dialog with a spinner and
the latest error. No manual Retry button needed — the loop retries
every few seconds. There is no cancel, no fallback to local mode.
The channel MUST be established before proceeding.

### 3. Fix `_popped` latch ordering

Set `_popped = true` only AFTER channel setup +
`setCoordinationUiEnabled` succeed, immediately before
`Navigator.pop(result)`.

## Verification

- `flutter analyze lib` clean
- Remote keygen → lands in remote wallet shell (not local)
- Transient relay failure → auto-retries, eventually succeeds
