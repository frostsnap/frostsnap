# NostrIdentity sum type + kill ambient publish snapshot

Replace the pair of "nsec: String" FFI param + ambient
`NostrClient.local_publish: Mutex<Option<(NostrProfile, String)>>`
snapshot with a single `NostrIdentity` sum type that carries both the
signing keys and the in-channel-profile distribution policy. Delete
the `set_local_publish_credentials` / `refreshPublishCredentials`
sync dance. Pass identity into `ChannelRunner` as a mandatory
constructor arg, snapshot at construction, and let all send methods
read it from there — no per-method identity plumbing.

Prerequisite for the shelved [[nostr_recovery_transport]] plan — its
entry points still had `nsec: String` params; this plan lets them
take `NostrIdentity` instead.

## The type

Lives in `frostsnap_nostr` (single source of truth; frostsnapp /
Dart mirror via FRB). Fields use `Nsec` (validated String wrapper
already FRB-exposed at `api/nostr/mod.rs:843-853`) rather than
`nostr_sdk::Keys` — `Keys` is opaque and can't be
non-opaque-mirrored across FRB. Same pattern the existing FFI
already uses everywhere: Dart passes `Nsec` / string at the
boundary, Rust parses to `Keys` on demand.

```rust
/// A signing identity plus the policy for publishing a profile to
/// peers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum NostrIdentity {
    /// User's nsec has a public NIP-01 kind 0 out on relays. Peers
    /// fetch the profile from the public network via the runner's
    /// existing profile-fetch path. We do NOT publish an encrypted
    /// in-channel kind 0.
    Imported {
        nsec: Nsec,
        /// The public profile as we last observed it — used for
        /// local UI display of "me" without an extra fetch. Not
        /// authoritative for peers, who read the public kind 0
        /// themselves.
        cached_profile: NostrProfile,
    },
    /// User's nsec is app-generated and has no public kind 0. We
    /// publish an encrypted kind 0 into every channel we join so
    /// peers see the name inside the channel via
    /// MemberProfileUpdated.
    Generated {
        nsec: Nsec,
        name: String,
        created_at: u64,
    },
}

impl NostrIdentity {
    /// Parse the nsec into signing `Keys`. Returns `Err` if the
    /// bech32 string is malformed — same failure mode the
    /// per-method `Keys::parse(&nsec)?` sites carry today.
    pub fn keys(&self) -> Result<Keys>;
    pub fn public_key(&self) -> Result<PublicKey>;
    /// The profile to publish encrypted into channels this identity
    /// joins. `Some` for `Generated` (unpublished elsewhere);
    /// `None` for `Imported` (already on public relays — never
    /// publish in-channel).
    pub fn in_channel_profile(&self) -> Option<NostrProfile>;
}
```

`Nsec` needs `Serialize + Deserialize` derives (for the persistence
migration below) — currently only has `Debug + Clone`. Add
`serde::Serialize + serde::Deserialize`.

Note on where `Nsec` lives: currently declared in
`frostsnapp/rust/src/api/nostr/mod.rs` (the FFI crate), not in
`frostsnap_nostr`. Moving `Nsec` down into `frostsnap_nostr` (with
a re-export at the FFI layer for compatibility) is part of this
plan — it's the honest place for a validated nsec wrapper if
`frostsnap_nostr` is the single source of truth for identity.

Alternative rejected: opaque `NostrIdentity` with Rust
constructors (`NostrIdentity::imported(nsec, profile) -> Result<Self>`).
Would work but hides variants from Dart pattern-matching + UI
display logic ("which mode am I in? show this settings row"). The
non-opaque variant is FRB-safe once all fields are FRB-safe types,
which `Nsec` + `NostrProfile` + `String` + `u64` are.

## Subsumes `UserIdentity` in `nostr_settings_state.rs`

The existing `UserIdentity` enum in
`frostsnapp/rust/src/nostr_settings_state.rs:15` is already the
same shape (`Imported { pubkey, cached_public_profile } |
Generated { pubkey, name, created_at }`) with the same
`in_channel_profile()` method — it just lacks the `nsec` field
(nsec lives separately in `NostrSettingsState.nsec: Option<String>`
as ambient state) and lives in the FFI crate.

`NostrIdentity` in `frostsnap_nostr` is what `UserIdentity` should
have been from the start: nsec bundled into each variant, single
source of truth, lives in the crate that owns nostr concepts.
This plan collapses:

- Delete `UserIdentity` from
  `frostsnapp/rust/src/nostr_settings_state.rs`. Replace usages
  with `NostrIdentity`.
- Delete the standalone `NostrSettingsState.nsec: Option<String>`
  field.
- Delete `NostrSettingsState.pubkey: Option<PublicKey>` — it was
  a cache; `NostrIdentity::public_key()` computes it on demand.
- Collapse `Mutation::SetNsec` and `Mutation::SetIdentity` into a
  single `Mutation::SetIdentity { identity: Option<NostrIdentity> }`.

`in_channel_profile()` migrates as a method on `NostrIdentity`
verbatim.

## Persistence: new format only (breaking change)

Existing DBs have two `nostr_settings` rows:

- `key = 'nsec'`, `value = <bech32>`
- `key = 'identity'`, `value = <UserIdentity as JSON>` (fields:
  `pubkey`, `cached_public_profile` OR `name` + `created_at`)

The new format stores a single row:

- `key = 'identity'`, `value = <NostrIdentity as JSON>` (fields
  now include `nsec` + `cached_profile` OR `name` + `created_at`).

No backward-compatibility migration, but load is
**tolerant of stale rows** — it must not block app startup, or
the user can't reach the setup flow to re-enter identity.
`Persist::load`:

1. Reads the `identity` row. Attempts
   `serde_json::from_str::<NostrIdentity>(&s)`. If that fails
   (old shape, corrupt, whatever), log a warning and treat as
   `identity: None`. The stale row is CLEARED (DELETEd) inside
   the load transaction so the next write starts from a clean
   slate.
2. Reads the `nsec` row (from the old format). If present,
   clears it — the new format doesn't use a separate nsec row.
3. Continues loading `nostr_access_structure_settings` regardless
   — access-structure preferences aren't identity-shaped and
   shouldn't be lost because identity load stumbled.

Net effect: existing installs land in state `identity: None` on
first boot after this plan, keep their per-access-structure
settings, and the Dart setup flow prompts for identity re-entry.
No user-visible startup failure; no data-loss for orthogonal
settings.

`Mutation::SetIdentity` apply also DELETEs any lingering `nsec`
row in the same transaction as a belt-and-suspenders normaliser
(covers the case where a stale nsec row survived load).

`Mutation::SetIdentity` apply writes the `identity` row with nsec
inside its JSON and DELETEs any lingering `nsec` row in the same
transaction — normalises persisted state to the new format on the
first identity mutation post-upgrade (relevant only if load
succeeded and a stale `nsec` row somehow persists — belt-and-
suspenders).

## Runner-owned signing + publish policy

The runner has TWO orthogonal concerns:

- **Signing**: needs a `Keys` to sign outgoing events. Every use
  case has one, whether it's the user's persistent nsec or an
  ephemeral protocol key.
- **In-channel profile publish**: only user-facing channels
  (chat, keygen lobby, remote signing, recovery lobby) want this.
  Keygen protocol subchannels use ephemeral keys with no user
  profile attached — they must not publish.

The runner's internal state reflects the split. Nothing stores a
full `NostrIdentity` past construction — we parse it into its
signing + publish parts and normalise once:

```rust
struct ChannelRunner {
    // ...existing fields (channel_keys, message_expiration, init_event)...
    signing: Option<SigningConfig>,   // set by exactly one of the two builders below
}

struct SigningConfig {
    keys: Keys,
    pubkey: PublicKey,                // cached from keys.public_key(); no re-derive
    publish_profile: Option<NostrProfile>,
    // ↑ `Some` for Generated identities (with pubkey field normalised
    // to `pubkey` above so equality against state.members entries is
    // structural, no false mismatch); `None` for Imported identities
    // AND for ephemeral-signing-keys use.
}

impl ChannelRunner {
    pub fn new(channel_keys: ChannelKeys) -> Self { /* signing: None */ }

    /// User-facing channels: chat, keygen lobby, remote signing,
    /// recovery lobby. Parses nsec once; if the identity is
    /// `Generated`, its `in_channel_profile()` is captured for the
    /// publish-after-fold check with its pubkey normalised.
    /// `Imported` identities produce `publish_profile: None` here
    /// (they never publish in-channel).
    ///
    /// Fallible: propagates `Nsec` parse errors — same failure
    /// mode today's per-method `Keys::parse(&nsec)?` sites carry.
    pub fn with_identity(mut self, identity: NostrIdentity) -> Result<Self> {
        let keys = identity.keys()?;
        let pubkey = identity.public_key()?;
        let publish_profile = identity.in_channel_profile().map(|mut p| {
            p.pubkey = Some(pubkey);  // normalise for structural equality
            p
        });
        self.signing = Some(SigningConfig { keys, pubkey, publish_profile });
        Ok(self)
    }

    /// Internal-only: keygen protocol subchannels sign with an
    /// ephemeral protocol Keys and must not publish any user
    /// profile. `pub(crate)` so the frostsnapp layer can't
    /// accidentally reach for it — Dart-side construction always
    /// goes through `with_identity`.
    pub(crate) fn with_ephemeral_signing_keys(mut self, keys: Keys) -> Self {
        let pubkey = keys.public_key().into();
        self.signing = Some(SigningConfig { keys, pubkey, publish_profile: None });
        self
    }
}
```

`run()` unwraps `signing` up front — that's a caller-bug guard,
not a user-reachable path:

```rust
pub async fn run(self, client: Client) -> Result<...> {
    let signing = self.signing.expect(
        "ChannelRunner::run called before with_identity or with_ephemeral_signing_keys",
    );
    // ...subscribe, fold, etc. Everything downstream reads
    //    `signing.keys` / `signing.pubkey` — no re-parse.
}
```

`ChannelRunnerHandle` stashes what downstream signers need
(`Keys` clone from `signing.keys` — nostr_sdk `Keys` is cheap
to clone). Send methods on `ChannelHandle` / lobby handles drop
the `nsec: String` param entirely and sign with the stashed keys.
Net effect: 11 methods lose their identity param, gain nothing.

### Publish-after-fold check

Inside `run()`, after subscribe + initial cache replay lands
historical events into `state.members`, the runner checks:

```rust
if let Some(desired) = &signing.publish_profile {
    let observed = state.lock().unwrap()
        .members
        .get(&signing.pubkey)
        .and_then(|slot| slot.profile.clone())
        .map(|mut p| { p.pubkey = Some(signing.pubkey); p });
    // ^ normalise observed's pubkey the same way desired's was
    //   normalised in with_identity; then equality is structural
    //   and won't spuriously mismatch on a differently-set pubkey
    //   field.
    if observed.as_ref() != Some(desired) {
        // spawn publish of desired using signing.keys
    }
}
```

Idempotent — only publishes when the fold doesn't already have
our current profile. Covers "user joins channel and just watches;
should still show up in peers' participant lists." Steady state
(own publish echoed back into fold): check is O(1) no-op.

Since the signing config is snapshotted at runner construction,
mid-session name changes do NOT propagate to existing channels
— user has to reconnect (or bounce the channel) to see the new
identity land. Explicitly out of scope; the reactive shared-Arc
alternative was rejected earlier. Future plan can add a
`handle.refresh_identity` method if needed.

### Why two builders instead of one signature

Rejected alternatives:

- **A** (make keygen synthesise `NostrIdentity::Imported` with a
  throwaway nsec + default profile): reuses `Imported` for a
  case that isn't imported. Variant name lies; awkward at the
  keygen subchannel construction sites.
- **C** (`Option<NostrIdentity>` on the runner): mixes two
  concerns into one Option — a `None` would mean either
  "ephemeral signing, no publish" or "no signing capability at
  all." Signing IS always required, so C can't compile cleanly.
  Also reverses the user's "mandatory identity" simplification.
- **D** (typed builder state: `ChannelRunner<Unconfigured>` vs
  `ChannelRunner<Ready>` at the type level): the cleanest
  compile-time guarantee but a bigger refactor than warranted;
  the runtime `expect` in `run()` matches the risk profile of
  the existing builder chain (no compile-time enforcement that
  `channel_keys` or `message_expiration` was set either).

Grep during impl for existing keygen subchannel construction
sites (should be a small number — probably in
`frostsnap_nostr::keygen::protocol`) and route them through
`with_ephemeral_signing_keys`.

## What gets deleted

- **`NostrClient.local_publish: Mutex<Option<(NostrProfile, String)>>`**
  (`api/nostr/mod.rs:612`) — the whole field.
- **`NostrClient::set_local_publish_credentials()`** — no more callers.
- **`NostrClient::spawn_lobby_publish_profile()`** — replaced by
  the runner-owned publish-after-fold path.
- **Dart `refreshPublishCredentials`** — call sites go away.
- **`nsec: String` params on 11 methods**: `ChannelHandle::send_message`,
  `send_receive_address`, `send_sign_request`,
  `send_test_sign_request`, `send_sign_offer`, `send_sign_partial`,
  `send_sign_cancel`, plus the async `NostrClient::create_remote_lobby`
  / `join_remote_lobby` / (upcoming) `create_remote_recovery_lobby`
  / `join_remote_recovery_lobby`. Not replaced — signing uses the
  runner's stashed Keys.
- **`UserIdentity` enum** in `nostr_settings_state.rs:15`.
- **`NostrSettingsState.nsec: Option<String>`** and
  **`NostrSettingsState.pubkey: Option<PublicKey>`**.
- **`Mutation::SetNsec` variant** — collapsed into `SetIdentity`.

## Entry-point signatures after the change

```rust
impl NostrClient {
    pub async fn connect_to_channel(
        &self,
        identity: NostrIdentity,       // NEW (was: implicit via local_publish)
        params: &ChannelConnectionParams,
    ) -> Result<ChannelHandle>;

    pub async fn create_remote_lobby(
        &self,
        identity: NostrIdentity,       // was: nsec: String + local_publish
        channel_secret: ChannelSecret,
        key_name: String,
        purpose: KeyPurpose,
    ) -> Result<RemoteLobbyHandle>;

    pub async fn join_remote_lobby(
        &self,
        identity: NostrIdentity,
        channel_secret: ChannelSecret,
    ) -> Result<RemoteLobbyHandle>;

    // ...and the two upcoming recovery entry points from
    // [[nostr_recovery_transport]] (unblocked by this plan).
}

impl ChannelHandle {
    // No identity/nsec on any of these — runner has it.
    pub async fn send_message(&self, content: String, reply_to: Option<EventId>) -> Result<EventId>;
    pub async fn send_receive_address(&self, derivation_index: u32, memo: String) -> Result<EventId>;
    // ...etc.
}
```

## Ephemeral outer envelope keys unchanged

The per-outer-event `Keys::generate()` used to sign the wrapping
NIP-44 envelope (for privacy from relays) stays exactly as today.
This plan touches only the identity keys that sign the inner
payload.

## FRB / Dart

`NostrIdentity` gets FRB-mirrored non-opaquely so Dart can
construct + pass it. The two variants and their fields map
directly. Dart-side, the identity-holding service reads persisted
state and constructs `NostrIdentity.Imported(nsec, cachedProfile)`
or `NostrIdentity.Generated(nsec, name, createdAt)` on demand,
handing it to every entry point.

The `refreshPublishCredentials` call sites just go away. Settings
pages that mutate identity write to persistent state; the next
join reads the current identity fresh.

## Migration order

1. Move `Nsec` from `frostsnapp/rust/src/api/nostr/mod.rs` down
   into `frostsnap_nostr` with `serde` derives (re-export at the
   FFI layer for compat).
2. Introduce `NostrIdentity` in `frostsnap_nostr`. No callers yet.
3. Update `ChannelRunner` to accept `NostrIdentity` (mandatory);
   parse to Keys once at construction; publish-after-fold check.
4. Update `ChannelHandle` send methods to drop `nsec: String`
   params — sign with the runner's stashed Keys.
5. Update `connect_to_channel` to take `NostrIdentity`.
6. Update `RemoteLobbyHandle` (keygen) analogously — the LOBBY
   channel goes through `with_identity`, using the user's
   `NostrIdentity`. The subchannels (in
   `frostsnap_nostr::keygen::protocol`) go through
   `with_ephemeral_signing_keys(protocol_keys)` — internal-only,
   no user profile.
7. Update `nostr_settings_state.rs`: swap `UserIdentity` →
   `NostrIdentity`; collapse `SetNsec` + `SetIdentity` mutations
   into single `SetIdentity`. Load reads only the new-format
   `identity` row and deserializes to `NostrIdentity`; a stale
   old-shape row surfaces as a deserialize error — install must
   re-run identity setup (breaking change, called out in release
   notes).
8. Update all Dart call sites: pass identity instead of nsec.
   Delete `refreshPublishCredentials`.
9. Delete `NostrClient.local_publish`,
   `set_local_publish_credentials`,
   `spawn_lobby_publish_profile`.
10. Delete `UserIdentity` and its `in_channel_profile()` impl
    from `nostr_settings_state.rs`.
11. Unshelve [[nostr_recovery_transport]] and update its recovery
    entry points to take `NostrIdentity`.

## Out of scope

- Reactive shared state between `NostrSettings` and `NostrClient`
  (`Arc<RwLock<NostrIdentity>>`).
- Merging `NostrSettings` and `NostrClient` into one struct.
- Mid-session identity refresh / profile propagation via
  per-method identity threading — dropped in this simplification;
  runner snapshot suffices.
- Backward-compatible load of the OLD (nsec + old-identity JSON)
  row shape. Pre-release breaking change; users re-enter identity.
- Changing outer envelope `Keys::generate()` logic.

## Verification

- `cargo check -p frostsnap_nostr -p rust_lib_frostsnapp` clean
  after each migration step (do it in order so the tree stays
  compilable).
- `just gen` regenerates FRB bindings clean.
- `flutter analyze lib` clean.
- Grep confirms zero remaining `nsec: String` params on
  `ChannelHandle` / `RemoteLobbyHandle` / `NostrClient` public
  surface.
- Grep confirms `local_publish` / `set_local_publish_credentials`
  / `refreshPublishCredentials` / `UserIdentity` / `SetNsec` are
  all deleted.
- Unit test in `nostr_settings_state`: load with new-format
  `identity` row → deserializes to `NostrIdentity`. Load with no
  identity row → `identity` is None, load succeeds. Load with
  old-shape identity JSON → `identity` is None, load succeeds,
  the stale row is DELETEd from the DB, and any pre-existing
  `nostr_access_structure_settings` rows load unaffected.
- Unit test (`frostsnap_nostr::channel_runner`): construct a
  runner with `NostrIdentity::Generated { ... }`; assert the
  publish-after-fold spawns a profile publish when state.members
  doesn't have us yet, and doesn't when it already does.
- Manual (via `just run-dual`): open chat, send message, verify
  peer sees my current name.
