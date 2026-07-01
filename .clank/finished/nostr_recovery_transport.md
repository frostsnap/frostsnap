# nostr_recovery_transport
# frostsnap_nostr: recovery transport

Wire layer that carries a remote recovery over a nostr channel,
mirroring `frostsnap_nostr/src/keygen/` and
`frostsnap_nostr/src/signing/`. Sits above the shared channel
infrastructure (per-consumer runner, encrypted-events, ChannelHandle),
and below the FRB API + Dart UI. Feeds the two-function core surface
(`RecoveringAccessStructure::new` +
`FrostCoordinator::finalize_remote_recovery`) that the previously
finalized `nostr_recovery` plan landed.

Independent of the UI plan
([[nostr_recovery_lobby_ui]] — separate work).

## Design premise

A recovery channel is a short-lived encrypted subchannel derived from
a per-session random `ChannelSecret` the leader generates. Joiners
receive the secret via an invite link (identical shape to keygen
`frostsnap_link:` URI). Each participant publishes one or more
`SharePost`s — a flat wire payload (see invariant #2). All
participants assemble those into `RecoverShare`s locally and run
fuzzy interpolation over the accumulated bundle; leader broadcasts
`Finish` when satisfied; non-leaders verify locally, then hop to a
signing channel derived from the recovered `AccessStructureRef`.

Channel messages carry a NIP-40 expiration of ~30 days (same shape
as `keygen::KEYGEN_MESSAGE_TTL`) so cooperating relays reap dead
recovery channels.

## Scope

- `frostsnap_nostr/src/recovery/` — new module, mirrors `keygen/`.
- `frostsnap_nostr/src/lib.rs` — re-exports.
- `frostsnap_nostr/src/channel.rs` — add
  `ChannelSecret::recovery_invite_link()` +
  `parse_recovery_link()` for the `frostsnap://recovery/<hex>`
  scheme (parallels the existing keygen link helpers).
- `frostsnapp/rust/src/api/nostr/mod.rs` — surface the two above
  to Dart via the existing extern-impl / trait pattern the
  keygen link helpers already use (`api/nostr/mod.rs:882-927`):
    - `#[frb(external)] impl ChannelSecret` gains a
      `pub fn recovery_invite_link(&self) -> String {}` entry.
    - `trait ChannelSecretExt` gains
      `fn from_recovery_link(link: &str) -> Result<ChannelSecret>;`
      and the matching impl calls `parse_recovery_link`.
    - The existing `ChannelSecretExt::generate()` covers fresh-
      secret generation — Dart uses it verbatim for the leader-
      creates-lobby flow rather than trying to call the RNG-
      taking native `ChannelSecret::random(&mut rng)`.
- Thin FRB wrapper in `frostsnapp/rust/src/api/nostr/remote_recovery.rs`
  — new file, mirrors `remote_keygen.rs` shape. Load-bearing core
  types on the wrapper's surface (`RecoverShare`, `HeldShare2`,
  `RecoveringAccessStructure`, `ShareImage`, `ShareCompatibility`,
  `AccessStructureRef`, `KeyPurpose`, `DeviceId`, `RestorationId`,
  `SymmetricKey`, `SharedKey`, `Fingerprint`) are already
  FRB-bridged via `api/recovery.rs` and `api/mod.rs` — inherited
  from the finalized `nostr_recovery` plan's inventory. Recovery-
  specific types (`SharePost`, `ObservedShare`, `ParticipantInfo`,
  `RecoveryChannelMetadata`, `RecoveredKey`, `FinishedRecovery`,
  `RecoveryLobbyState`) need new `#[frb(mirror(_), non_opaque)]`
  declarations added by this plan — see the "New FRB mirrors
  required" table under the FRB wrapper section below.

Dart-side pages and UX are the sister plan
([[nostr_recovery_lobby_ui]]).

## Design invariants

1. **Channel secret is a fresh random per session, held by the
   leader.** Joiners get it in the invite link. Same shape as
   `KeygenLobbyClient::new(channel_secret)` at
   `frostsnap_nostr/src/keygen/lobby.rs:450`.
2. **Wire carries a flat `SharePost` — NOT `RecoverShare`.**
   `SharePost { device_id, device_name, device_kind, share_image,
   needs_consolidation }`. Decoupling from the frostsnap_core
   `RecoverShare` / `HeldShare2` types means core can evolve
   without breaking protocol compatibility, and the untrusted-
   metadata concern becomes "the wire schema simply doesn't carry
   those fields" rather than "strip them on decode." Includes
   device name + kind — same treatment as keygen's
   `DeviceRegistration` (`keygen/lobby.rs:102`), so the group can
   show "Alice's Frostsnap-A is contributing share X" attributions.
3. **Fold constructs `RecoverShare` from `SharePost` for the
   fuzzy pass.** The wrapper lifts each `SharePost` into
   `RecoverShare { held_by: post.device_id, held_share: HeldShare2 {
   share_image: post.share_image, needs_consolidation:
   post.needs_consolidation, access_structure_ref: None, threshold:
   None, key_name: None, purpose: None } }` — the `None`s are the
   seatbelt against
   `RecoveringAccessStructure::access_structure_ref()`'s
   metadata-fallback trap (invariant #6 from the finalized
   `nostr_recovery` plan). Since the wire schema doesn't have
   those fields in the first place, they can only ever be `None`
   at this layer.
4. **Leader / non-leader distinction lives only at the wire
   layer.** The core `RecoveringAccessStructure::new` and
   `finalize_remote_recovery` don't care. Wire enforces:
   `Finish` and `CancelLobby` from a non-leader author are
   dropped by the fold.
5. **Reuse the existing channel infrastructure end-to-end.** New
   module composes `ChannelClient` +
   `ChannelHandle` (per-consumer runner, listen-then-start
   contract, close/Drop lifecycle from `simplify_channel_shutdown`)
   — no new low-level primitives.

## Module layout

### `frostsnap_nostr/src/recovery/mod.rs`

```rust
pub mod lobby;
pub use lobby::{
    RecoveryLobbyClient, RecoveryLobbyHandle, RecoveryLobbyMessage,
    RecoveryLobbyState, RecoveryLobbyEvent, RecoveryChannelMetadata,
    ParticipantInfo as RecoveryParticipantInfo,
};
pub const RECOVERY_MESSAGE_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
```

### `frostsnap_nostr/src/recovery/lobby.rs`

Wire messages:

```rust
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum RecoveryLobbyMessage {
    Presence,
    /// Contribute a share to the pool. The wire payload is a flat
    /// `SharePost` — deliberately NOT the frostsnap_core `RecoverShare`
    /// type. Decoupling the wire schema from a core-crate type means
    /// core can evolve `RecoverShare`/`HeldShare2` without breaking
    /// protocol compatibility (invariant #2). The fold assembles a
    /// `RecoverShare` from `SharePost` internally when calling
    /// `RecoveringAccessStructure::new`, filling the untrusted
    /// `HeldShare2` fields (`access_structure_ref`, `threshold`,
    /// `key_name`, `purpose`) with `None` since the wire schema
    /// simply doesn't carry them (invariant #3).
    Share(SharePost),
    Finish { share_refs: Vec<EventId> },
    Leave,
    CancelLobby,
}

/// Flat wire payload for one contributed share. Same on-the-wire on
/// every participant's screen (device name / kind are shared so the
/// group can display "Alice's Frostsnap-A" style attributions — same
/// approach keygen takes via `DeviceRegistration { device_id, name,
/// kind }` at `keygen/lobby.rs:102`).
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SharePost {
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_kind: DeviceKind,           // reuse `crate::keygen::lobby::DeviceKind`
    pub share_image: ShareImage,
    pub needs_consolidation: bool,
}

/// Leader-authored, bincode-encoded and base64-wrapped into the
/// NIP-28 `ChannelCreation` event's `content` field — the same
/// pattern keygen uses (`frostsnap_nostr/src/keygen/lobby.rs:77`
/// with `LobbyChannelMetadata::encode_content` /
/// `decode_content`). Immutable for the life of the channel;
/// joiners paint the wallet-name / purpose / threshold-hint as
/// soon as the ChannelCreation event lands, with no separate
/// round-trip.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct RecoveryChannelMetadata {
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold_hint: Option<u16>,
}

impl RecoveryChannelMetadata {
    /// Bincode → base64. Matches keygen's `LobbyChannelMetadata::encode_content`.
    pub fn encode_content(&self) -> Result<String>;
    /// base64 → bincode. Matches keygen's `LobbyChannelMetadata::decode_content`.
    pub fn decode_content(content: &str) -> Result<Self>;
}
```

The ChannelCreation event itself is built by
`RecoveryLobbyClient::build_creation_event(&self, identity:
&NostrIdentity) -> Result<InnerEvent>` (see the client section
below): sets `kind: Kind::ChannelCreation`,
`content: metadata.encode_content()?`, signs with
`identity.keys()?`. Joiners' fold reads the ChannelCreation via the
runner's `state_arc.creation_event`, decodes its content, and
populates `RecoveryLobbyState.metadata`.

Folded state:

```rust
/// One SharePost as observed in the fold: the wire `SharePost` payload
/// plus the nostr envelope (event id, author) that carried it. Same
/// `SharePost` payload defined above under wire messages — the fold
/// re-uses the wire type as-is, augmented with its envelope.
pub struct ObservedShare {
    pub event_id: EventId,
    pub author: PublicKey,
    pub post: SharePost,
}

pub struct ParticipantInfo {
    pub pubkey: PublicKey,
    pub joined_at: Timestamp,
    pub profile: Option<NostrProfile>,
    pub posted_shares: Vec<EventId>,
    pub left: bool,
}

pub struct RecoveryLobbyState {
    // Leader identity is NOT stored here — it's the author of the
    // NIP-28 ChannelCreation event, already tracked by the underlying
    // channel runner in `ChannelState.creation_event: Option<Event>`
    // (`channel_runner.rs:120`). The fold rules that need to check
    // authority (Finish / CancelLobby) read
    // `creation_event.map(|e| PublicKey::from(e.pubkey))` from the
    // shared `state_arc` the wrapper task holds — same access pattern
    // simplify_channel_shutdown established. Storing it in our own
    // fold state would duplicate a single source of truth. (Keygen's
    // `LobbyState.initiator` at `keygen/lobby.rs:153` does duplicate
    // it; that's an existing wart, not a pattern to copy.)
    //
    // `metadata` is non-Option for the same reason: it comes from the
    // ChannelCreation event's content field, which is the FIRST thing
    // that has to land for the channel to mean anything. The wrapper
    // task GATES `RecoveryLobbyEvent::StateChanged` emission on
    // `creation_event.is_some() && metadata_decode.is_ok()` — Dart
    // never sees a RecoveryLobbyState with unknown metadata. Any wire
    // messages that arrive before the ChannelCreation are buffered
    // internally and folded after decode succeeds. Pre-metadata
    // "connecting" UI states are expressed via the runner's separate
    // `ConnectionState::Connecting`/`Connected` events, not via this
    // struct.
    pub metadata: RecoveryChannelMetadata,
    pub participants: HashMap<PublicKey, ParticipantInfo>,
    pub shares: BTreeMap<EventId, ObservedShare>,
    pub current_recovery: Option<RecoveredKey>,
    pub finished: Option<FinishedRecovery>,
    pub cancelled: bool,
}

pub struct RecoveredKey {
    pub access_structure_ref: AccessStructureRef,
    pub winning_share_refs: Vec<EventId>,
    // shared_key kept inside Rust; not exposed
}

pub struct FinishedRecovery {
    pub access_structure_ref: AccessStructureRef,
    pub share_refs: Vec<EventId>,
    // shared_key kept inside Rust
}

pub enum RecoveryLobbyEvent {
    StateChanged(RecoveryLobbyState),
    RecoveryAvailable(RecoveredKey),
    Finished(FinishedRecovery),
    FinishVerificationFailed,
    Cancelled,
}
```

Client + handle (parallels `LobbyClient` / `LobbyHandle` in keygen):

```rust
pub struct RecoveryLobbyClient { ... }

impl RecoveryLobbyClient {
    pub fn new(channel_secret: ChannelSecret) -> Self;
    pub fn with_metadata(mut self, meta: RecoveryChannelMetadata) -> Self;
    pub fn invite_link(&self) -> String;
    /// Build the NIP-28 ChannelCreation InnerEvent signed with
    /// `identity.keys()`. Content is the encoded
    /// `RecoveryChannelMetadata` (already stashed via
    /// `with_metadata`). Leader-only.
    pub async fn build_creation_event(&self, identity: &NostrIdentity) -> Result<InnerEvent>;

    /// Spawn the per-lobby runner. Forwards `identity` to
    /// `ChannelRunner::with_identity(identity)?` — that call
    /// captures the signing Keys AND the in-channel profile publish
    /// policy (Imported: skip, Generated: publish-after-fold). The
    /// resulting handle carries the stashed Keys internally so
    /// downstream `RecoveryLobbyHandle` send methods don't need to
    /// re-thread them.
    pub async fn run(
        self,
        client: Client,
        identity: NostrIdentity,
        init_event: Option<Event>,
        sink: impl Sink<RecoveryLobbyEvent> + Clone + Sync,
    ) -> Result<RecoveryLobbyHandle>;
}

#[derive(Clone)]
pub struct RecoveryLobbyHandle {
    runner_handle: ChannelRunnerHandle,
}

impl RecoveryLobbyHandle {
    /// Publish methods sign with `runner_handle.signing_keys()` —
    /// the Keys stashed at `RecoveryLobbyClient::run` time via
    /// `ChannelRunner::with_identity(...)`. No per-call keys param;
    /// same collapse the chat-side `ChannelHandle` did post-
    /// [[nostr_identity_type]].
    pub async fn announce_presence(&self) -> Result<SendOutcome>;
    pub async fn post_share(&self, post: SharePost) -> Result<SendOutcome>;
    pub async fn finish(&self, share_refs: Vec<EventId>) -> Result<SendOutcome>;
    pub async fn leave(&self) -> Result<SendOutcome>;
    pub async fn cancel_lobby(&self) -> Result<SendOutcome>;
}
```

Fold rules (in `process_event`). The wrapper task holds `state_arc:
Arc<Mutex<ChannelState>>` (per the `simplify_channel_shutdown`
pattern) so it can read the runner's `creation_event` on each event
to identify the leader without duplicating state.

**Emission gate.** Before `ChannelState.creation_event.is_some()`
AND the decoded `RecoveryChannelMetadata` is available, the wrapper
task buffers incoming events internally (in a `Vec<Event>` — bounded
in practice by the relay's replay window) and emits NO
`RecoveryLobbyEvent::StateChanged`. When the ChannelCreation event
arrives, the wrapper decodes its content into
`RecoveryChannelMetadata`; on success, it drains the buffer,
applies each buffered event to the fold, and emits the first
`StateChanged` with metadata populated. On decode failure the
wrapper emits `Cancelled` (channel is malformed) and shuts down.
This keeps `RecoveryLobbyState.metadata: RecoveryChannelMetadata`
non-Option — Dart never sees an unknown-metadata state.

**FRB entry-point interaction with the gate.** The FRB layer's
`join_remote_recovery_lobby` needs an initial `RecoveryLobbyState`
to seed the `BehaviorBroadcast<RecoveryLobbyState>` it hands to
Dart, and can't seed with a Default (metadata is non-Option). The
sink adapter (`RecoveryBridgeSink`) is the single, long-lived event
consumer that:

1. Is constructed BEFORE `RecoveryLobbyClient::run`, holding
   `Arc<Mutex<...>>` for all its internal state — no swap, no
   handoff, no second sink.
2. Holds `broadcast: Arc<Mutex<Option<BehaviorBroadcast<RecoveryLobbyState>>>>`
   — `None` at construction. On the FIRST `StateChanged(state)`
   event: the sink lazily populates it with
   `BehaviorBroadcast::seeded(state)` and fires an
   `Arc<Notify>` (`ready`). Every subsequent `StateChanged`
   forwards through `broadcast.as_ref().unwrap().add(&state)`.
3. Holds `finished_slot`, `verification_failed`, and
   `state_changed` `Arc`s exactly as before — populated from the
   corresponding `Finished` / `FinishVerificationFailed` /
   `RecoveryAvailable` events regardless of whether the broadcast
   is initialized yet (the `finished_slot` write is a mutex swap,
   not routed via the broadcast).
4. Holds `cancelled_pre_state: Arc<Mutex<bool>>` — set on
   `Cancelled` iff the broadcast is still `None`. In that same
   branch the sink ALSO calls `ready.notify_waiters()` (the same
   wake path first-StateChanged uses) so a joiner parked in the
   arm-then-check loop below wakes immediately and returns the
   malformed-channel error, instead of sitting until the 30s
   timeout. Post-first-StateChanged Cancelled writes the fold's
   `state.cancelled = true` via the normal broadcast update path
   — no separate wake needed there.

Entry-point flow (joiner). Uses the same arm-then-check pattern
`remote_keygen::await_keygen_ready` uses (`remote_keygen.rs:403-410`)
so a `notify_waiters` fired between the sink observing first-emit
and the entry point calling `.notified()` isn't lost — pin +
`enable()` primes the Notified future before we check the state,
so any subsequent notify wakes it whether the wait was armed or not.

```rust
let bridge = RecoveryBridge::new();   // all Arcs default/None
let sink = bridge.sink();             // Arc-clones the shared state
let inner = lobby_client
    .run(client, identity.clone(), None, sink)
    .await?;

let broadcast = tokio::time::timeout(Duration::from_secs(30), async {
    loop {
        // Arm before we check state (TOCTOU guard, mirrors
        // remote_keygen.rs:403-410).
        let notified = bridge.ready.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        if *bridge.cancelled_pre_state.lock().unwrap() {
            return Err(anyhow!("channel malformed — metadata decode failed"));
        }
        // CLONE, don't take() — BehaviorBroadcast: Clone is
        // internal Arc-share. Draining via take() would leave
        // the sink's slot as None and cause subsequent
        // StateChanged events to unwrap-panic or fork.
        if let Some(broadcast) =
            bridge.broadcast.lock().unwrap().as_ref().cloned()
        {
            return Ok(broadcast);
        }

        notified.await;
    }
})
.await
.map_err(|_| anyhow!("ChannelCreation didn't arrive within 30s"))??;

Ok(RemoteRecoveryLobbyHandle::from_bridge(inner, keys, invite_link,
                                           client, broadcast, bridge))
```

Because the sink adapter is `Arc`-shared and long-lived, no event
can be lost between "created" and "handle constructed" — the sink
processes them as they arrive; the entry point just parks until
the sink says "first StateChanged has landed."

The leader entry point skips the await entirely: the leader owns
the metadata at call time, so it constructs the seed state
synchronously (empty participants/shares, metadata populated),
pre-populates the sink's broadcast with `BehaviorBroadcast::seeded(seed_state)`
BEFORE calling `run()`, and returns the handle immediately. The
leader's own `ChannelCreation` still publishes and lands via the
runner; the wrapper's first `StateChanged` fires and the sink
handles it via the "broadcast is Some → forward" branch. Same
type flow, different init.

- `Presence` → upsert `ParticipantInfo`.
- `Share(post)` from author `P` with event id `E` → wrap as
  `ObservedShare { event_id: E, author: P, post: post.clone() }` and
  insert into `shares`; append `E` to
  `participants[P].posted_shares`. Then build a
  `Vec<RecoverShare>` for the fuzzy pass by lifting each
  `ObservedShare.post` into a `RecoverShare { held_by:
  post.device_id, held_share: HeldShare2 { share_image:
  post.share_image, needs_consolidation: post.needs_consolidation,
  access_structure_ref: None, threshold: None, key_name: None,
  purpose: None } }` — the `None`s are the "strip untrusted
  metadata" invariant #3 (nothing in the SharePost wire schema
  can carry those fields in the first place; this is the
  seatbelt). Run `RecoveringAccessStructure::new` over that
  vector with `threshold_hint` from metadata +
  `Fingerprint::default()`; update `current_recovery` from the
  resulting `.shared_key` + `.compatibility()`-filtered winning
  subset.
- `Finish { share_refs }` from author `A` — read `leader = state_arc
  .lock().creation_event.map(|e| e.pubkey.into())`; drop if
  `Some(A) != leader`. Otherwise collect those specific shares from
  the bundle, run `RecoveringAccessStructure::new` over ONLY that
  subset with metadata's threshold + `Fingerprint::default()`; if
  `.shared_key.is_some()`, set `finished = Some(_)` and emit
  `Finished`; else emit `FinishVerificationFailed` and leave
  `finished` at `None`. Latched.
- `Leave` from `P` → mark `participants[P].left = true`; keep
  `posted_shares` intact (published data isn't retracted).
- `CancelLobby` from author `A` — same leader check as `Finish`;
  drop if not from leader. Otherwise set `cancelled = true`; emit
  `Cancelled`. Latched.

If the ChannelCreation event hasn't arrived yet (leader unknown),
`Finish` and `CancelLobby` are dropped defensively — leader
authority can't be verified against absent state.

## FRB wrapper: `frostsnapp/rust/src/api/nostr/remote_recovery.rs`

Mirrors `remote_keygen.rs` shape. Core types like `RecoverShare`,
`HeldShare2`, `ShareImage`, `RecoveringAccessStructure`, `KeyPurpose`,
`DeviceId`, `AccessStructureRef`, `SymmetricKey` are already mirrored
via `api/recovery.rs` and `api/mod.rs` (see the finalized
`nostr_recovery` plan's inventory).

### New FRB mirrors required by this plan

Every recovery-specific type on the fold-state surface needs an
`#[frb(mirror(_), non_opaque)]` declaration so Dart can consume the
`BehaviorBroadcast<RecoveryLobbyState>` snapshots. `DeviceKind` is
already mirrored via keygen (`api/nostr/remote_keygen.rs:28`) — reuse
that mirror; do NOT re-declare it here or FRB codegen collides.

| New mirror | Type source | Notes |
|---|---|---|
| `_SharePost` | `frostsnap_nostr::recovery::SharePost` | Flat wire payload; non-opaque; reuses already-mirrored `DeviceId`, `DeviceKind`, `ShareImage` |
| `_ObservedShare` | `frostsnap_nostr::recovery::ObservedShare` | Fold-side wrapper: `{event_id, author, post: SharePost}` |
| `_ParticipantInfo` | `frostsnap_nostr::recovery::ParticipantInfo` | `{pubkey, joined_at_secs, profile, posted_shares, left}` |
| `_RecoveryChannelMetadata` | `frostsnap_nostr::recovery::RecoveryChannelMetadata` | Non-opaque; the ChannelCreation content decoded |
| `_RecoveredKey` | `frostsnap_nostr::recovery::RecoveredKey` | `{access_structure_ref, winning_share_refs}` — no SharedKey field, that stays inside Rust |
| `_FinishedRecovery` | `frostsnap_nostr::recovery::FinishedRecovery` | `{access_structure_ref, share_refs}` — same "SharedKey inside Rust" discipline |
| `_RecoveryLobbyState` | `frostsnap_nostr::recovery::RecoveryLobbyState` | Container; `shares` field's collection type may need to be `Vec<ObservedShare>` at this layer rather than `BTreeMap<EventId, ObservedShare>` if FRB codegen doesn't handle the map — decide during impl and update the plan then |

`RecoveryLobbyMessage` is a wire-only enum (bincode encoded/decoded
inside the fold, never crosses the FRB boundary) — no mirror needed.
`RecoveryLobbyEvent` similarly stays Rust-internal; the FRB layer's
bridge sink translates it into `RecoveryLobbyState` broadcast updates
+ `finished_slot` writes.

```rust
broadcast_handle! { pub struct RecoveryLobbyStateBcast(pub BehaviorBroadcast<RecoveryLobbyState>); }

#[frb(opaque)]
pub struct RemoteRecoveryLobbyHandle {
    inner: RecoveryLobbyHandle,
    keys: Keys,
    invite_link: String,
    state_broadcast: BehaviorBroadcast<RecoveryLobbyState>,
    client: Client,
    finished_slot: Arc<Mutex<Option<(FinishedRecovery, SharedKey)>>>,
    state_changed: Arc<Notify>,
}

impl RemoteRecoveryLobbyHandle {
    #[frb(sync)] pub fn invite_link(&self) -> String;
    #[frb(sync)] pub fn my_pubkey(&self) -> PublicKey;
    #[frb(sync)] pub fn sub_state(&self) -> RecoveryLobbyStateBcast;
    pub async fn announce_presence(&self) -> Result<()>;
    pub async fn post_share(&self, post: SharePost) -> Result<EventId>;
    pub async fn finish(&self, share_refs: Vec<EventId>) -> Result<()>;
    pub async fn leave(&self) -> Result<()>;
    pub async fn cancel(&self) -> Result<()>;

    /// Await Finished (or FinishVerificationFailed → Err). Idempotent —
    /// resolves immediately if already finished.
    pub async fn await_finished(&self) -> Result<FinishedRecovery>;

    /// Persist the recovered access structure into the coordinator.
    /// Reads the sealed `SharedKey` out of `finished_slot`, calls
    /// `Coordinator::finalize_remote_recovery` with
    /// `my_local_devices` derived from the participant's own posts.
    /// Returns the recovered `AccessStructureRef`. Errors if not
    /// finished yet.
    pub async fn persist_recovered(
        &self,
        coord: &Coordinator,
        encryption_key: SymmetricKey,
    ) -> Result<AccessStructureRef>;
}
```

`NostrClient` entry points — same shape as the existing
`create_remote_lobby` / `join_remote_lobby` for keygen post-
[[nostr_identity_type]]: caller supplies the `ChannelSecret`
explicitly and a `NostrIdentity` (obtained via
`NostrContext.nostrSettings.currentIdentity()` on Dart, the same
value that flows into `connect_to_channel` and the keygen lobby
entry points). No `nsec: String` at this layer — validation lives in
`Nsec`'s constructor. Dart obtains `ChannelSecretExt::generate()`
for creation (the FRB-exposed wrapper — see Scope section) and
`ChannelSecretExt::from_recovery_link(url)` for joining; the
invite link is exposed via `RemoteRecoveryLobbyHandle::invite_link()`
for the leader to share.

```rust
impl NostrClient {
    /// Leader entry: caller pre-generates a fresh `ChannelSecret`
    /// (from Dart: `ChannelSecretExt::generate()`) and provides
    /// their `NostrIdentity` (from
    /// `NostrSettings::currentIdentity()`). The `ChannelCreation`
    /// event is signed with `identity.keys()`; its content carries
    /// `RecoveryChannelMetadata` (name, purpose, threshold hint)
    /// encoded. The underlying
    /// `ChannelRunner::with_identity(identity)` also captures the
    /// identity's in-channel profile publish policy — Imported
    /// participants skip in-channel publish, Generated ones
    /// publish-after-fold per the shared publish-check landed in
    /// [[nostr_identity_type]]. Returns a handle immediately
    /// (leader already knows metadata) whose `invite_link()` yields
    /// the `frostsnap://recovery/<hex>` URL to share with joiners.
    pub async fn create_remote_recovery_lobby(
        &self,
        identity: NostrIdentity,
        channel_secret: ChannelSecret,
        key_name: String,
        purpose: KeyPurpose,
        threshold_hint: Option<u16>,
    ) -> Result<RemoteRecoveryLobbyHandle>;

    /// Joiner entry: caller has parsed the leader's invite link
    /// (from Dart: `ChannelSecretExt::from_recovery_link(link)`)
    /// and provides their own `NostrIdentity`. Same
    /// with_identity(identity) forwarding as above, so publish
    /// semantics are identical. Awaits the leader's
    /// `ChannelCreation` event (30s timeout) before returning, so
    /// the handle's `BehaviorBroadcast` seed carries real metadata;
    /// errors on timeout or on a `Cancelled` before the first
    /// `StateChanged` (malformed metadata).
    pub async fn join_remote_recovery_lobby(
        &self,
        identity: NostrIdentity,
        channel_secret: ChannelSecret,
    ) -> Result<RemoteRecoveryLobbyHandle>;
}
```

## NostrIdentity integration (post-`nostr_identity_type` rewrite)

Delta from the shelved version:

- **Entry-point args**: `nsec: String` → `identity: NostrIdentity` on
  both `create_remote_recovery_lobby` and
  `join_remote_recovery_lobby` (see signatures above). No per-call
  nsec parsing at this layer — identity is validated at construction
  by [[nostr_identity_type]]'s validating `Nsec::Deserialize` /
  `Nsec::parse`.
- **`RemoteRecoveryLobbyHandle` publish methods** (`post_share`,
  `finish`, `cancel_lobby`, `leave`) — same collapse the chat-side
  `ChannelHandle` send methods did in the identity plan: drop the
  `keys: &Keys` param, sign with the runner's stashed
  `runner_handle.signing_keys()`. Callers get one less thing to
  thread through.
- **`RecoveryLobbyClient::run`** — takes `identity: NostrIdentity` and
  forwards to `ChannelRunner::with_identity(identity)?`. The
  runner-owned publish-after-fold check (idempotent, defined in
  [[nostr_identity_type]]) handles the "Generated user joined, make
  them visible" case at zero extra cost here.
- **No `spawn_lobby_publish_profile` call site.** That function was
  deleted in the identity plan; the runner-owned publish supersedes.
- **FRB mirror inventory delta**: unchanged from the shelved plan's
  §"new-FRB-mirror inventory" — `NostrIdentity` and `Nsec` are
  already mirrored (non-opaque) by the identity plan, so this plan
  adds only the recovery-specific mirrors (`SharePost`,
  `ParticipantInfo`, `RecoveryChannelMetadata`, etc.). Delete the
  entry for a `Nsec`-shaped mirror from the shelved inventory if it
  was listed — done there.
- **Dart-side callers** obtain the identity via
  `NostrContext.nostrSettings.currentIdentity()` (same shape
  chat_page and org_keygen_page use now — see the identity plan's
  Dart call sites for the exact pattern).

## Out of scope (for this plan)

- Dart pages and UI — separate plan [[nostr_recovery_lobby_ui]].
- Post-recovery signing channel hop — named as follow-up in the
  finalized `nostr_recovery` plan; implemented later.
- Multi-leader / cross-channel coordination — different secrets,
  different channels; no cross-channel state.
- Adversarial leader mitigations beyond local `Finish` verification.

## Tests

At least one end-to-end integration test in
`frostsnap_nostr/tests/recovery_live.rs` (matches the shape of
`signing_live.rs` / `keygen_live.rs`) that:

1. Runs a keygen to produce N valid share backups (fixture).
2. Spins up leader + 2 joiner participants, each with their own
   `NostrClient` and per-consumer `ChannelHandle` (via the
   existing channel machinery).
3. Leader creates the recovery lobby; joiners join via invite.
4. Each participant posts a `SharePost` built from a fixture
   backup (device_id + fixture device_name + `DeviceKind::Frostsnap`
   + backup's share_image + `needs_consolidation: true`).
5. Leader observes `RecoveryAvailable`, publishes `Finish` with
   the winning refs.
6. Non-leaders observe `Finished`; call `persist_recovered` on a
   fresh coordinator; assert `AccessStructureRef` matches fixture.
7. Assert that publishing `Finish` with a bogus subset (e.g.
   swapped share_image bytes) produces `FinishVerificationFailed`
   on non-leaders and no persistence.

## Verification

- `cargo check -p frostsnap_nostr -p rust_lib_frostsnapp`.
- `cargo test -p frostsnap_nostr --test recovery_live`.
- `just gen` regenerates FRB bindings clean.
- `flutter analyze lib` clean (the FRB additions surface in Dart).
