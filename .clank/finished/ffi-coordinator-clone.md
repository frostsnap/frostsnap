# Make FfiCoordinator Clone

## Problem

`LoopContext` in `keygen_run.rs` clones 8 individual `Arc` fields
from `FfiCoordinator` because the spawned tokio task needs `'static`
ownership. This duplicates finalize side effects (emit key state,
backup run setup, backup stream emit) and creates a maintenance
hazard.

## Fix

### 1. Make FfiCoordinator Clone

Wrap the two non-Clone fields in `Arc`:

- `usb_manager: Mutex<Option<UsbSerialManager>>` → `Arc<Mutex<...>>`
- `thread_handle: Mutex<Option<JoinHandle<()>>>` → `Arc<Mutex<...>>`

All other fields are already `Arc<Mutex<...>>` or `Clone`.
Derive `Clone` on `FfiCoordinator`.

### 2. Shrink LoopContext

Replace the 8 duplicated `Arc` fields with one `FfiCoordinator`
clone. Keep session-local fields that don't belong on
`FfiCoordinator`:

```rust
struct RemoteKeygenSession {
    coord: FfiCoordinator,
    keygen_id: KeygenId,
    keys: Keys,
    local_devices: BTreeSet<DeviceId>,
    participants: Vec<SelectedParticipant>,
}
```

### 3. Add FfiCoordinator helper for remote finalize

Add a method on `FfiCoordinator` that handles the shared
post-finalize side effects:

```rust
impl FfiCoordinator {
    /// Run finalize_remote_keygen + shared post-finalize side effects.
    /// Returns (AccessStructureRef, full device→share map).
    /// Does NOT build ChannelParticipant (no nostr pubkeys here).
    pub(crate) fn finalize_remote_keygen_with_side_effects(
        &self,
        keygen_id: KeygenId,
        encryption_key: SymmetricKey,
    ) -> Result<(AccessStructureRef, BTreeMap<DeviceId, ShareIndex>)> {
        // 1. staged_mutate: coord.finalize_remote_keygen(...)
        // 2. Extract device_to_share_index + local_devices from result
        // 3. usb_sender.send_from_core(finalized)
        // 4. kg.keygen_finalized(asr) — BEFORE any fallible post-finalize
        // 5. emit_key_state()
        // --- everything below is best-effort, no ? ---
        // 6. start backup run (silent on error)
        // 7. backup stream emit (silent if no subscriber)
        // Returns (access_structure_ref, device_to_share_index)
    }
}
```

**Ordering**: `keygen_finalized(asr)` fires BEFORE backup setup.
If backup fails, the UI protocol is already finished — cleanup
won't turn a finalized keygen into an aborted state.
`FfiCoordinator` can manage `RemoteKeyGen` UI protocol (it's
coordinator/UI state, not nostr state).

### 4. RemoteKeygenSession::finalize builds RemoteKeygenResult

```rust
impl RemoteKeygenSession {
    fn finalize(&self, encryption_key: SymmetricKey) -> Result<RemoteKeygenResult> {
        let (asr, device_map) = self.coord
            .finalize_remote_keygen_with_side_effects(self.keygen_id, encryption_key)?;
        // Cross-reference device_map with self.participants
        // to build Vec<ChannelParticipant>
        Ok(RemoteKeygenResult { access_structure_ref: asr, participants })
    }
}
```

`FfiCoordinator` never sees nostr pubkeys or `ChannelParticipant`.

## What stays the same

- `FfiCoordinator` API surface (existing methods unchanged)
- Local keygen path (untouched)
- `drain_outgoing` stays on the session context (handles
  `CoordinatorSend::Broadcast` which is nostr-specific)
- Backup stream semantics: best-effort emit, silent if no subscriber

## Verification

- `cargo check --workspace` clean
- `flutter analyze lib` clean
- `LoopContext` deleted (or renamed to `RemoteKeygenSession`)
- No individual `Arc` field clones from `FfiCoordinator`
- Remote keygen finalize side effects are in one place
