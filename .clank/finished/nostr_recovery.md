# frostsnap_core: remote_recovery module

Third leg of the remote-coordination stack in `frostsnap_core`,
alongside `coordinator/remote_keygen.rs` and
`coordinator/remote_signing.rs`. Scope of THIS plan is **only** the
new `frostsnap_core/src/coordinator/remote_recovery.rs` module plus
one constructor added to `RecoveringAccessStructure` in
`restoration.rs`. Nostr wire protocol, FRB additions, and Dart pages
are separate plans.

## Design premise

Remote recovery aggregates share-holding `HeldShare2` payloads from
N remote participants over some external transport (a nostr channel,
in practice, but this module doesn't know or care). The transport
IS the source of truth for which shares exist AND for which of them
need consolidation; frostsnap_core stores no per-recovery state
between calls. The API is **pure stateless functions**: build a
`RecoveringAccessStructure` from a bundle of shares, then persist
via `FrostCoordinator::finalize_remote_recovery`.

Contrast with `remote_keygen.rs`, which is heavily stateful
(`State { active_keygens: BTreeMap<KeygenId, RemoteKeygenState> }`,
multi-phase state machine per keygen). Remote recovery has no
protocol-level state to keep — fuzzy interpolation is a pure
function of the input set at any moment in time.

## Design invariants

1. **Reuse `RecoveringAccessStructure` as the fold engine.** The
   type in `restoration.rs:1466` already has `add_share`,
   `compatibility`, `access_structure_ref()`, `shared_key`,
   `compatible_device_to_share_index()`, `needs_to_consolidate()`.
   Remote recovery adds one constructor on it and one persistence
   method on `FrostCoordinator`. Nothing else new in core.
2. **Reuse FRB-bridged types on the public surface.** Every type
   the public surface takes or returns is already bridged for the
   local recovery flow (see FRB inventory below). Downstream
   (nostr layer + FRB layer + Dart) inherits Dart-side rendering
   for free.
3. **Fingerprint is a parameter to `RecoveringAccessStructure::new`.**
   The plan initially specified hardcoding `Fingerprint::FROST_V0`,
   but the existing test harness (`test/mod.rs:44`) sets a weaker
   `TEST_FINGERPRINT` on both `FrostCoordinator.keygen_fingerprint`
   and each `FrostSigner` to avoid grinding cost in the test suite.
   A hardcoded FROST_V0 in `::new` breaks against test wallets.
   The parameter matches the existing `add_share(share, fingerprint)`
   shape. Production callers pass `Fingerprint::default()`
   (= `Fingerprint::FROST_V0`); tests pass `TEST_FINGERPRINT`.
   `finalize_remote_recovery` doesn't need it — it only reads
   `.shared_key` from an already-populated RAS.
4. **Wire carries `RecoverShare` (bincode-encoded).** That
   struct is `{ held_by: DeviceId, held_share: HeldShare2 }` —
   already bincode-derived, already FRB-mirrored. `held_by` is
   the DeviceId of the device that will hold the share locally
   after recovery finishes: for plugged-in devices, that device's
   own `DeviceId`; for paper-backup entries, the DeviceId of the
   participant's device the backup will be saved into (local
   recovery already requires this via
   `tell_device_to_save_physical_backup`, so the app already has
   it by the time the participant is ready to publish).

   Load-bearing HeldShare2 fields on the wire: `share_image` (the
   crypto) and `needs_consolidation` (drives the consolidation
   filter at finalize time — see invariant #5 below). Other
   HeldShare2 fields (`access_structure_ref`, `threshold`,
   `key_name`, `purpose`) the nostr layer should set to `None`
   since peer-supplied metadata is untrusted (see invariant #6).

   Publishing `needs_consolidation` exposes "am I restoring from
   a paper backup?" to the channel — a small privacy trade for
   a real simplification: no local persisted state between
   finalize calls, no separate "consolidation intent" input to
   `finalize_remote_recovery`.

   Multiple shares per participant are fine — each has its own
   event with its own `held_by` DeviceId (a participant can hold
   multiple devices). The `held_by → ShareIndex` mapping stays
   1:1 as long as no device holds two share indices for the same
   wallet, which the existing local flow already assumes.
5. **Consolidation filter is local at finalize time.**
   `finalize_remote_recovery` derives
   `devices_needing_consolidation = recovered.needs_to_consolidate()
   ∩ my_local_devices`. The wire told us WHICH shares need
   consolidation via `HeldShare2.needs_consolidation`; the caller
   tells `finalize_remote_recovery` WHICH DEVICES are theirs via
   `my_local_devices` — intersection is applied inside so we don't
   try to consolidate another participant's device.
6. **Never call `RecoveringAccessStructure::access_structure_ref()`
   in the remote context** (codex catch from a prior revision). Its
   implementation (`restoration.rs:1497`) falls back to
   `HeldShare2.access_structure_ref` metadata on the input shares
   when `shared_key` is None. That metadata is untrusted from
   peers — a peer can tag their offered share with any ASref. Use
   `.shared_key.is_some()` for "did this reconstruct?" checks and
   `AccessStructureRef::from_root_shared_key(&shared_key)` for the
   resulting ASref. The nostr layer should further construct its
   input `HeldShare2`s with `access_structure_ref: None` regardless
   of what the wire carried, so the fallback is unreachable.
7. **Device layer untouched.** Zero device-side changes.

## Module layout

New file: `frostsnap_core/src/coordinator/remote_recovery.rs`.

Register in `frostsnap_core/src/coordinator.rs`:

```rust
pub mod remote_keygen;
pub mod remote_recovery;   // NEW
pub mod remote_signing;
```

No new field on `FrostCoordinator`.

One-line addition in `frostsnap_core/src/coordinator/restoration.rs`:
the `RecoveringAccessStructure::new` constructor (see below). This
belongs on the type itself, not in the remote_recovery module.

## Public surface

Two additions. That's the whole thing.

### `RecoveringAccessStructure::new` (in `restoration.rs`)

```rust
impl RecoveringAccessStructure {
    /// Fold a bundle of shares into a fresh `RecoveringAccessStructure`.
    /// Equivalent to constructing an empty one and calling `add_share`
    /// per input against `Fingerprint::FROST_V0`. Convenience for
    /// batch callers (remote recovery, tests) — the local flow
    /// continues to accumulate shares via the mutation stream.
    pub fn new(
        shares: &[RecoverShare],
        starting_threshold: Option<u16>,
    ) -> Self {
        let mut ras = Self {
            starting_threshold,
            held_shares: vec![],
            shared_key: None,
        };
        for share in shares {
            ras.add_share(share.clone(), Fingerprint::FROST_V0);
        }
        ras
    }
}
```

Non-leader verify at the call site (in the nostr layer, later plan):

```rust
let ras = RecoveringAccessStructure::new(&subset, threshold_hint);
if ras.shared_key.is_none() { /* leader lied; abort */ }
```

Leader "find a winning subset" at the call site:

```rust
let ras = RecoveringAccessStructure::new(&all_shares, threshold_hint);
if ras.shared_key.is_some() {
    let winning: Vec<&RecoverShare> = ras.held_shares.iter()
        .filter(|s| ras.compatibility(s.held_share.share_image) == ShareCompatibility::Compatible)
        .collect();
    // publish winning share event-refs in Finish
}
```

No dedicated `verify_subset_reconstructs`, no `recover_from_shares`
free function — the type's constructor + existing accessors are
enough.

### `finalize_remote_recovery` (on `FrostCoordinator`, in `remote_recovery.rs`)

```rust
impl super::FrostCoordinator {
    /// Persist a fully-reconstructed access structure obtained via
    /// remote recovery. Mirrors the tail of
    /// `restoration::finish_restoring:659-682` — calls
    /// `mutate_new_key`, records `DeviceNeedsConsolidation` for
    /// devices that both (a) appear in `recovered.needs_to_consolidate()`
    /// AND (b) are in `my_local_devices` — but bypasses the
    /// `RestorationState` lifecycle entirely (no `RestorationId`,
    /// no `start_restoring_key`, no `add_recovery_share_to_restoration`).
    ///
    /// Returns `Err(RestorationError::NotEnoughShares)` when
    /// `recovered.shared_key.is_none()` — the bundle hasn't
    /// reconstructed yet, so there's nothing to persist. Matches
    /// the local `finish_restoring:649` shape (same variant, same
    /// meaning). Callers who've already checked
    /// `.shared_key.is_some()` at the call site can `.unwrap()`.
    ///
    /// `my_local_devices` is the caller's own device set (devices
    /// this participant contributed shares from). Filters BOTH:
    ///
    /// - the `device_to_share_index` map handed to `mutate_new_key`
    ///   — `mutate_new_key` emits `KeyMutation::NewShare` for every
    ///   entry (`coordinator.rs:1428`), and claiming to hold a
    ///   remote participant's encrypted share in our own
    ///   coordinator state would lie to downstream signing /
    ///   recovery / backup code paths that walk the access
    ///   structure. Same split the remote keygen path takes at
    ///   `remote_keygen.rs:646`.
    /// - the `needs_to_consolidate()` iterator, so we don't try
    ///   to consolidate another participant's device.
    ///
    /// The wallet-global "which share index does device X hold?"
    /// mapping is NOT persisted in our coordinator for non-local
    /// devices; each participant's coordinator only records its
    /// own share bindings. If a participant later reconnects with
    /// a device that isn't in their local set, the standard
    /// discovery flow picks it up.
    ///
    /// `key_name` and `purpose` come from the leader-authored
    /// channel metadata (the caller has already fetched them
    /// from the transport). They are not derivable from the
    /// shared_key alone.
    pub fn finalize_remote_recovery(
        &mut self,
        recovered: &RecoveringAccessStructure,
        key_name: String,
        purpose: KeyPurpose,
        my_local_devices: &BTreeSet<DeviceId>,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<AccessStructureRef, RestorationError>;
}
```

Implementation (~20 lines):

```rust
let root_shared_key = recovered.shared_key.as_ref()
    .ok_or(RestorationError::NotEnoughShares)?
    .clone();
let full_map = recovered
    .compatible_device_to_share_index()
    .expect("shared_key is Some ⇒ compatible_device_to_share_index returns Some");

// Filter to LOCAL devices before mutate_new_key. Passing the full
// winning-subset map would emit KeyMutation::NewShare for remote
// participants' devices too (coordinator.rs:1428) — lying about
// what shares we hold. Matches the remote_keygen split at
// remote_keygen.rs:646.
let local_map: BTreeMap<DeviceId, ShareIndex> = full_map
    .iter()
    .filter(|(d, _)| my_local_devices.contains(d))
    .map(|(d, i)| (*d, *i))
    .collect();

let access_structure_ref = self.mutate_new_key(
    key_name, root_shared_key, local_map,
    encryption_key, purpose, rng,
);

for device_id in recovered.needs_to_consolidate() {
    if !my_local_devices.contains(&device_id) { continue; }
    self.mutate(Mutation::Restoration(RestorationMutation::DeviceNeedsConsolidation(
        PendingConsolidation {
            device_id,
            access_structure_ref,
            share_index: full_map[&device_id],
        },
    )));
}

Ok(access_structure_ref)
```

Skips the terminal `DeleteRestoration` mutation — no restoration
exists.

Lives on `FrostCoordinator` (not on the module's types) for the
same reason `add_key_and_access_structure` lives on
`FrostCoordinator` in `remote_keygen.rs:353` — it's a persistence
op, not a protocol op.

## FRB-bridged type inventory

The FRB API surface for this module is inherited from
`frostsnapp/rust/src/api/recovery.rs`. Every type the public
functions take or return is already bridged:

| Type | Where bridged | Kind |
|---|---|---|
| `RecoverShare` | `api/recovery.rs:272` | mirror (non-opaque) |
| `HeldShare2` | `api/recovery.rs:278` | mirror (non-opaque) |
| `RecoveringAccessStructure` | `api/recovery.rs:306` | mirror (non-opaque) |
| `ShareCompatibility` | `api/recovery.rs:339` | mirror (enum) |
| `AccessStructureRef` | `api/mod.rs:231` | mirror (non-opaque) |
| `KeyPurpose` | `api/mod.rs` (via `use crate::api::KeyPurpose`) | mirror |
| `DeviceId` | `api/mod.rs:88` | mirror |
| `RestorationId` | `api/mod.rs:238` | mirror |
| `SymmetricKey` | `api/mod.rs:262` | mirror |
| `SharedKey` | opaque (auto-bridged), already used in `_RecoveringAccessStructure.shared_key` | opaque |
| `ShareImage` | opaque (auto-bridged), already used in `_HeldShare2.share_image` | opaque |
| `ShareIndex` | already flows through `restoration.rs` bridged methods | opaque |
| `Fingerprint` | `api/recovery.rs:294` in `_RestorationState.fingerprint` | mirror |

**No new FRB mirrors required for this module.** The follow-up
`api/nostr/remote_recovery.rs` plan will add a thin wrapper
exposing `RecoveringAccessStructure::new` and
`finalize_remote_recovery` to Dart, but every argument and return
type already round-trips.

The Dart side inherits, for free:
- `RecoveringAccessStructure.effective_threshold()` sync getter.
- `RestorationStatus.share_count()` (usable if the nostr layer
  wants to project into the local recovery UI's shape).
- `ShareCompatibility` enum for per-share filtering.

## What this module deliberately does NOT do

- No transport (nostr, or any other). Callers hand in
  `&RecoveringAccessStructure`; how they built the bundle isn't
  this module's problem.
- No `recover_from_shares` free function. `RecoveringAccessStructure::new`
  IS that; giving it a second name is just noise.
- No `verify_subset_reconstructs` helper. Non-leader verify is
  `RecoveringAccessStructure::new(subset, hint).shared_key.is_some()`
  — one line at the call site, no dedicated function warranted.
- No leader/non-leader roles baked in. Leader uses
  `.shared_key.is_some()` + `.compatibility()` to pick a winning
  subset; non-leader uses `.shared_key.is_some()` to accept.
  Distinction lives in the transport.
- No message-id / event-id / channel-secret concepts.
- No `expected: AccessStructureRef` argument anywhere. Finish
  enumerates share event-refs only; validity == "these shares
  reconstruct to some key." The reconstructed ASref is derived
  from the shared_key at finalize time.
- No local-vs-remote share source tracking. The wire tells us
  which shares need consolidation via `HeldShare2.needs_consolidation`;
  the caller tells `finalize_remote_recovery` which devices are
  theirs via `my_local_devices` — intersection is applied inside.

## Tests

**One end-to-end test in `frostsnap_core/tests/remote_recovery.rs`:**
`end_to_end_local_device_consolidates_share`. It captures the whole
arc a local participant actually walks through, and everything else
is either exercised by the walk or covered by `restoration.rs`'s own
tests.

The test:
1. Runs a real 2-of-3 keygen as a fixture, extracts three
   `ShareBackup`s (the wallet's canonical shares).
2. Wipes the coordinator, creates a single blank device.
3. Has the blank device enter its physical backup — the share lands
   in the device's `tmp_loaded_backups` (no `start_restoring_key`,
   no local restoration mutations).
4. Composes a `RecoverShare` bundle: the local device's share plus
   two "remote" shares with fresh DeviceIds standing in for
   `nostr_pubkey_to_device_id(peer_pubkey)`.
5. `RecoveringAccessStructure::new(&shares, Some(2), TEST_FINGERPRINT)`
   — asserts `.shared_key.is_some()`.
6. `finalize_remote_recovery(&ras, ..., my_local_devices={local}, ...)`
   — persists the wallet, queues `PendingConsolidation` for
   `local_device` only.
7. `consolidate_pending_physical_backups(local_device, ...)` — drives
   the device through its `Consolidate` handler; device extracts the
   secret against the recovered `SharedKey` and stores an encrypted
   `CompleteSecretShare`.

Asserts, all against the real coordinator and device state:
- The recovered `AccessStructureRef` matches the fixture wallet's.
- `has_backups_that_need_to_be_consolidated(local_device)` was true
  before consolidation (covers the `PendingConsolidation` filter).
- `knows_about_share(local_device, asref, local_share_index)` after
  consolidation — coordinator records the local device's share.
- `device.get_encrypted_share(asref, local_share_index)` — device
  holds a working encrypted share.
- `knows_about_share(remote_device_a, ...)` and
  `knows_about_share(remote_device_b, ...)` are both false — we
  don't record remote participants' devices as our own share
  holders (covers the `mutate_new_key` device-map filter, i.e. the
  `remote_keygen.rs:646`-style split).

**What we DON'T test here (and why):**
- `RecoveringAccessStructure::new` in isolation (zero shares,
  sub-threshold, threshold-hint sensitivity, `.compatibility()`
  per-share reports, metadata-fallback quirk). The `::new`
  constructor is a thin `add_share` loop over a type whose
  `add_share`/`try_fuzzy_recovery` are already exercised by
  `tests/restoration.rs` (the local recovery flow adds shares one
  at a time via `apply_mutation_restoration:312` and asserts the
  same outcomes). Duplicating those cases against `::new`
  would test the loop, not the recovery logic. The end-to-end
  test above hits `::new` with a real 3-share bundle and asserts
  `.shared_key.is_some()`, which is what we actually depend on.
- The metadata-fallback trap on `.access_structure_ref()` remains
  documented in invariant #6 and the doc comment on `finalize_remote_recovery` /
  the module-level doc of `remote_recovery.rs`. Callers know not
  to use it. A dedicated regression test is warranted only if
  downstream code starts calling `access_structure_ref()` for
  reconstruction checks — which the wire protocol / nostr layer
  plan (§Follow-up) will explicitly avoid. If that plan
  regresses, the guard lives there.
- `finalize_remote_recovery` `Err(NotEnoughShares)` path. The
  code returns before any `self.mutate(...)` calls (the `?` is
  the very first statement), and the invariant is stated in the
  doc comment. Testing "returns the right variant on this input"
  duplicates the compiler's job. If a future refactor moves
  mutation calls before the `?`, that regression would show up
  in the end-to-end test (via an unexpected access structure) or
  can be added at that point.
- `my_local_devices = {}` case. Legitimate use case (coordinator-only
  participant) but it changes exactly one call path (no
  `NewShare`/`DeviceNeedsConsolidation` mutations emitted, key
  still persists). That path is covered by the end-to-end test's
  device-filter assertions — the "local" and "remote" branches
  of the same iterator. If empty-local ever needed its own test,
  add it then.

## Verification

- `cargo test -p frostsnap_core --test remote_recovery` passes.
- `cargo test -p frostsnap_core` passes (existing `restoration.rs`
  tests untouched).
- `cargo check -p frostsnap_core` clean.
- `remote_keygen.rs` / `remote_signing.rs` unchanged.

## Follow-up plans (not this one)

- `frostsnap_nostr/src/recovery/` — wire messages (carries
  `RecoverShare` per invariant #4), lobby state fold,
  `RecoveryLobbyClient` / `RecoveryLobbyHandle`. Constructs
  each `RecoverShare` such that its inner `HeldShare2` has
  `access_structure_ref: None` (per invariant #6) to keep the
  ASref-fallback unreachable.
- `frostsnapp/rust/src/api/nostr/remote_recovery.rs` — thin FRB
  wrapper + `NostrClient::create_remote_recovery_lobby` /
  `join_remote_recovery_lobby`. Trivial: types already bridged.
- Dart pages: recovery lobby UI, "enter backup" flow, participant
  list, Finish button, post-recovery signing hop. Reuses local-
  recovery widgets that render `RecoveringAccessStructure` /
  `RestorationStatus`.
