# nostr_profile
# Nostr profile in channel

## Status of the prior implementation (commits `ba4dfc2..b332765`)

Done and kept:
- Storage layer: `UserIdentity` sum type, serde derives,
  identity-mode-aware API on `NostrSettings`.
- Generate flow asks for the name up front (setup dialog and
  advanced page).
- Mode A import gate via 30 s public-kind-0 fetch.
- Runner-level fold of in-channel `Kind::Metadata` into
  `ChannelState.members` with strict-greater `created_at` and
  in-channel-wins precedence.
- `NostrClient.local_publish` snapshot +
  `set_local_publish_credentials` setter; chat page now uses
  the shared `NostrContext.nostrClient`.
- Mode A's read-only profile card + Refresh from relays.

Broken or wrong (this revision addresses):
- **Profile publish doesn't reach the keygen lobby.**
  `publish_profile_in_all_channels` iterates only
  `NostrClient.channels` (chat handles). The lobby uses
  `RemoteLobbyHandle` / `LobbyHandle`, in a separate place.
  Peers in the lobby never see your name.
- **Profile publish lives at the wrong layer.**
  `ChannelHandle::publish_profile` is implemented in
  `signing/mod.rs`. The lobby wrapper has no equivalent. The
  whole publish/dedup mechanism is a property of the encrypted
  channel and must live on `ChannelRunnerHandle`, shared by
  both wrappers.
- **Lobby UI doesn't render profile data at all.**
  `_ParticipantRow` shows a custom circle + short hex pubkey
  for peers; `b332765` only fixed the self label. Even when
  profile data exists (e.g. Mode A's `cached_public_profile`
  with a picture URL), the lobby never displays it.
- **Mode B name editor was a misstep.** It implied
  "save propagates to active channels," which required a
  cross-channel iteration scheme. That's overkill for what we
  actually need. The name is now set ONCE — at identity
  generation — and never editable in-app afterwards. Renaming
  is a future plan that can deal with invalidation semantics
  properly. Tearing out the editor lets us delete the publish-
  everywhere path entirely.

## What the user wants

A way to set a human-friendly name for display in group channels.
Users are in exactly one of two identity modes — the mode is
chosen at nsec acquisition time and constrains the whole profile
UX.

Mode A users (imported nsec) keep whatever profile picture their
public kind 0 already has — the app just displays it. Mode B
users (app-generated nsec) get **name only**; profile pictures
are intentionally not supported because hosting them would
require either (a) inline data URLs (non-conventional, bloats
event payloads above relay caps, doesn't render in other nostr
clients) or (b) a full Blossom upload + crop pipeline (real
work, deferred to its own plan). If a Mode B user wants a
picture, the answer is "import an nsec whose public profile has
one" — i.e., switch to Mode A.

### Mode A — imported nsec (public identity)

User imported an existing nsec via "Import different nsec." Their
public identity already exists on the nostr network as a NIP-01
kind 0 event managed by their other clients. The app uses the
nsec for in-channel signing and **never publishes any kind 0
event**. Peers in channels see their name (and picture, if their
public kind 0 has one) by fetching the public kind 0 via the
existing peer-fetch path
(`channel_runner::spawn_profile_fetch`).

The app enforces that imported nsecs **must** have a discoverable
kind 0 at import time. An nsec with no public profile is rejected
with a clear message asking the user to set one up in their usual
nostr client first.

### Mode B — app-generated nsec (in-channel identity)

User let the app generate a fresh nsec ("Generate new random
identity"). The Generate flow REQUIRES a display name up front
— that name is the user's identity. The identity is
**private-by-default**: it never gets a public kind 0.

The name is **immutable after creation in this revision.**
Profile Settings shows it read-only. To use a different name
the user generates a new identity (new nsec) via the existing
"Generate new random identity" button. In-app renaming is
deferred to a future plan that can address invalidation
semantics across cached profiles and historical channels.

In-channel publish mechanism:
- Identity is persisted locally with its name.
- Encrypted `Kind::Metadata` event published inside each channel
  on connect-to-channel. That's the ONLY publish trigger.
- Channel runner folds in-channel kind 0 events into
  `ChannelState.members`.

These are the only two modes. There is no "do both." A user who
wants to switch modes uses the existing identity-management
buttons (`Generate new`, `Import different`, `Remove`) —
switching clears the previous mode's storage.

## "Encrypted channel" means every channel, including the lobby

The original stub said: *"This will set up a kind 0 profile in
the encrypted channel (and will send it again to any encrypted
channel you join)."*

There are two kinds of encrypted channel in this app:

1. **Chat channel** — `frostsnap_nostr::signing::ChannelClient`
   → `ChannelHandle`, registered in `NostrClient.channels`.
   Survives keygen and persists for the lifetime of the wallet.
2. **Keygen lobby channel** — `frostsnap_nostr::keygen::LobbyClient`
   → `LobbyHandle` (wrapped by `RemoteLobbyHandle` in the FRB
   layer). Short-lived; exists only while a keygen is being
   coordinated.

Both wrap the same `ChannelRunner` (with the same encryption,
the same `h`-tag scoping, and — after the runner work in
`ba4dfc2` — the same `Kind::Metadata` fold). The Mode-B publish
path MUST hit both. Showing a name only in the chat after keygen
defeats the point: the moment your peers most need to recognise
you is during the keygen lobby, before the wallet exists.

Both channel types also need to surface profile data to their
respective Dart UIs:

- Chat: already wired — `chat_page` listens for
  `ChannelEvent::MemberProfileUpdated` and renders via
  `NostrAvatar`.
- Lobby: **not wired** — `_ParticipantRow` in
  `org_keygen_page.dart` renders a custom circle (initials /
  star icon) and a hex-shortened pubkey for peers. Needs to
  switch to `NostrAvatar` + a profile cache backed by the
  lobby's `MemberProfileUpdated` stream.

## Wire model

Inside the encrypted channel, publish standard NIP-01 metadata
events (`Kind::Metadata` aka kind 0). The channel runner already
wraps every inner event in NIP-44 encryption and broadcasts via
the channel's `h`-tag; kind 0 inner events ride the same
mechanism — no new event kind invented.

Content is the canonical NIP-01 metadata JSON. For Mode B
in-channel publishes, only `name` is set:

```json
{ "name": "Lloyd" }
```

The `NostrProfile` struct retains its other fields (`picture`,
`display_name`, `about`, `banner`, `nip05`, `website`) so that
Mode A's fetched public profiles round-trip them for display.
The Mode B editor doesn't touch any of them.

## Storage semantics

Local DB stores **one** identity record per app install. The
record is a sum type tagged by identity mode:

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "mode")]
enum UserIdentity {
    /// Mode A — public identity managed elsewhere. We persist
    /// just the pubkey + a cached snapshot of the public kind 0
    /// for offline display. Refreshed opportunistically when
    /// the runner sees an updated kind 0 in the wild.
    Imported {
        pubkey: PublicKey,
        cached_public_profile: Option<NostrProfile>,
    },

    /// Mode B — in-channel identity. Name is captured at
    /// generation time and immutable thereafter; the connect-time
    /// publish trigger reads it. `created_at` is the Unix-second
    /// stamp from the generation moment, useful only for display
    /// (e.g. "identity created on …") since the name doesn't
    /// change.
    Generated {
        pubkey: PublicKey,
        name: String,
        created_at: u64,
    },
}
```

Schema: a single `user_identity` row holding a JSON-serialized
`UserIdentity` payload. Switching modes (via the existing
identity-management buttons) replaces the row in one transaction.

```sql
CREATE TABLE user_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 0),
    payload TEXT NOT NULL              -- JSON-encoded UserIdentity
);
```

(`singleton` enforces single-row table.)

No per-channel state stored. "Have we published to channel X?"
is answered from the runner's in-memory `ChannelState.members`
map (see Duplicate-suppression rules below) — the runner already
decrypts and folds in-channel kind 0 events into that map for
every member, so the local user's last-published profile is just
`state.members[local_pubkey]`.

Persisted in the existing sqlite database the coordinator
already opens. New migration adds the table.

### Identity acquisition

Two entry points produce a `UserIdentity`:

**`generate_new_identity(name: String) -> String`** (returns nsec)
- Validate `name` is non-empty after trim (caller already
  enforces this in the Generate dialog; reject defensively
  here too).
- Generate a fresh nsec via `Keys::generate()`.
- Write `UserIdentity::Generated { pubkey, name, created_at:
  now }`.
- Return the nsec. No network call needed.

**`import_identity(nsec)`** — gated by public-kind-0 discovery:
- Parse nsec → `Keys`. Derive `pubkey`.
- Check the local lmdb cache via the existing
  `get_cached_profile(client, pubkey)`. If found → accept,
  write `UserIdentity::Imported { pubkey, cached_public_profile:
  Some(...) }`.
- Otherwise call `fetch_metadata` with a **30-second timeout**
  (longer than the runtime's 5s `PROFILE_FETCH_TIMEOUT` because
  this is a one-time user-blocking import). If found → accept.
- If still not found → **reject the import** with the message:
  *"No public profile found for this nsec. Set one up in your
  usual nostr client (publish a kind 0 metadata event), then
  try importing again."*

The 30s constant lives next to `PROFILE_FETCH_TIMEOUT` as
`IMPORT_PROFILE_FETCH_TIMEOUT`.

### Equality semantics (Mode B publish path)

`NostrProfile` gains `derive(PartialEq, Eq)` on its
content-bearing fields (`pubkey`, `name`, `display_name`, `about`,
`picture`, `banner`, `nip05`, `website`). Source / ordering
metadata live on the runner-internal `MemberSlot` (see Fold
behavior), not on `NostrProfile` itself, so the derived `Eq` is
exactly the dedup comparison the publish path wants.

The publish path runs the dedup as:

```rust
let current = state.members.get(&local_pubkey).and_then(|s| s.as_ref());
match current {
    Some(slot) if slot.profile == profile_to_publish => SKIP,
    _ => publish,
}
```

`pubkey: Option<PublicKey>` quirk: when publishing,
`profile_to_publish.pubkey` must be set to `Some(local_pubkey)`
so the eq check matches the stored slot's profile (which always
has `Some(author)`). The publish path populates `pubkey` from
`keys.public_key()` before the comparison.

## Fold behavior on the runner

### Runner-internal slot type

To track ordering and source without mutating the wire-facing
`NostrProfile` type, the runner wraps each member's profile in a
runner-internal `MemberSlot`:

```rust
struct MemberSlot {
    profile: NostrProfile,
    source: ProfileSource,
    /// For source = InChannel only: the source event's
    /// `created_at`. Used for the strict-greater-wins ordering
    /// rule during cache replay. `None` for External.
    inchannel_created_at: Option<u64>,
}

enum ProfileSource {
    /// Decrypted from an in-channel `Kind::Metadata` event.
    InChannel,
    /// Fetched via the existing `spawn_profile_fetch` path
    /// (`get_cached_profile` → relay `fetch_metadata`).
    External,
}
```

`ChannelState.members` becomes `HashMap<PublicKey,
Option<MemberSlot>>`. `NostrProfile` itself stays unchanged: it
mirrors `nostr_sdk::Metadata` (content-bearing fields only), so
external fetches that don't expose a source-event timestamp
still construct a `NostrProfile` cleanly.

### Fold rule for in-channel kind 0

`channel_runner::process_event` extends to recognize inner events
with `kind == Kind::Metadata`:

- Parse content as `nostr_sdk::Metadata` → `NostrProfile`.
- Read `inner_event.created_at` from the decrypted inner event
  (the event itself carries the timestamp; we don't need to put
  it on `NostrProfile`).
- Apply a **strict-greater `created_at` rule** to handle
  out-of-order cache replay:
  - If `members[author]` is `None` OR currently `Some(slot)` with
    `slot.source == External` → replace with
    `Some(MemberSlot { profile, source: InChannel,
    inchannel_created_at: Some(inner_event.created_at) })`.
  - If `Some(slot)` with `slot.source == InChannel`:
    - If `inner_event.created_at > slot.inchannel_created_at.unwrap()`
      → replace (this is the in-channel update path).
    - Otherwise → drop (stale or equal-timestamp duplicate).
- Only emit `ChannelRunnerEvent::MemberProfileUpdated {
  pubkey, profile }` when the slot actually changed.

### Precedence vs the existing public-kind-0 fetch

`channel_runner.rs::process_inner_event` today calls
`spawn_profile_fetch(author, ...)` on first observation of any
new author. That fetch retrieves the author's PUBLIC `Kind::Metadata`
event (cache → relays) and writes via `profile_tx` to the same
`state.members[author]` slot.

**Precedence rule: in-channel wins.** The user explicitly
published in-channel for this group; honor that over a stale
public kind 0.

Mechanically, the external-fetch path:
- Constructs `MemberSlot { profile, source: External,
  inchannel_created_at: None }`.
- Writes ONLY if `members[author]` is `None` or
  `Some(slot)` with `slot.source == External` (refresh of an
  existing external entry — overwrite is fine, no ordering to
  preserve).
- If the current slot has `source == InChannel`, the external
  result is dropped silently. Once a member has an in-channel
  profile, public kind 0 changes won't override it for the
  duration of the session.

### Signing-layer event

The runner's `MemberProfileUpdated` maps to a new public variant
on `ChannelEvent`:

```rust
ChannelEvent::MemberProfileUpdated {
    pubkey: PublicKey,
    profile: NostrProfile,
}
```

Per-member granularity, distinct from today's
`ChannelEvent::GroupMetadata { members: Vec<GroupMember> }`
which fires once on bulk group-state updates. Consumers handle
`MemberProfileUpdated` for incremental per-author changes;
`GroupMetadata` continues to fire on creation-event /
participant-list changes. Don't collapse them.

Dart consumes it identically to today's `GroupMetadata`
update path — `NostrContext::updateProfilesFromChannel` already
threads profiles to chat-bubble renderers; route the per-member
event into the same cache.

## Publish trigger (Mode B only)

There's exactly one publish trigger: **the user joins a
channel** — chat or lobby — and the connect path fires
`publish_profile` on that channel's `ChannelRunnerHandle`. No
cross-channel iteration, no registry, no `publish_everywhere`
method, no Save button.

The connect-time hook reads `NostrClient.local_publish`. In
Mode A the snapshot is `None` and the publish is a no-op
(Mode A never publishes in-channel). In Mode B the snapshot
is always `Some` — name is required at identity creation —
so the hook always sends one encrypted kind 0 to the
just-joined channel; per-channel dedup inside `publish_profile`
skips if `state.members[self_pubkey]` already matches.

Implementation: a single tokio task spawned inside the
connect path after the wrapper's handle is constructed:

```rust
// inside connect_to_channel / create_remote_lobby / join_remote_lobby
if let Some((profile, nsec)) = self.local_publish.lock().unwrap().clone() {
    let h = runner_handle.clone();
    tokio::spawn(async move {
        if let Ok(keys) = Keys::parse(&nsec) {
            let _ = h.publish_profile(profile, &keys).await;
        }
    });
}
```

**Known race**: the runner has no `CacheReplayComplete` signal
today (same gap `runner-emits-tx-correlation-hints`
acknowledged and deferred). When the trigger fires immediately
after connect, the cached previous publish may not yet be
folded into `state.members` — so the dedup check sees `None`
and publishes anew. Net effect: **at most one extra metadata
event per app restart per channel**. Relays dedupe by event id
and last-`created_at`-wins downstream — UX stays correct.
Accept for the MVP.

## Duplicate-suppression rules

Channel events live encrypted in the lmdb cache as
`Kind::Custom(4)` outer wrappers with ephemeral outer authors and
an `h` tag; the inner `Kind::Metadata`, real author, and JSON
content are only visible after decryption. A direct DB filter on
inner fields therefore CANNOT work.

Instead, dedup uses the runner's existing in-memory
`ChannelState.members` map. The runner's `process_event` fold
already decrypts the encrypted outer wrapper, recognizes inner
`Kind::Metadata` events, and writes
`members[inner_author] = Some(MemberSlot { profile, source:
InChannel, inchannel_created_at: Some(inner_event.created_at) })`
per the Fold rule. By the end of cache replay this map reflects
the latest profile observed in the channel for every member —
including the local user, if they've published before.

The runner itself doesn't need to know which pubkey is "self";
the publish path provides that.
`ChannelRunnerHandle::publish_profile` takes the local
`keys: &Keys`, computes `local_pubkey = keys.public_key()`,
and looks up the local user's profile via its own single-author
accessor `member_profile(&local_pubkey)` (which projects the
slot back to just the `NostrProfile`):

1. Let `current = self.member_profile(&local_pubkey)`.
2. If `Some(existing)` and `existing == profile_to_publish`
   (the derived `NostrProfile` `PartialEq`) → SKIP.
3. Otherwise → publish, and trust the runner's own fold to
   update `members[local_pubkey]` with a fresh `MemberSlot`
   when the just-published event echoes back from the relay
   subscription.

This means the "have I already published this in this channel?"
question is answered from in-memory state the runner already
maintains for the general `members` purpose, not from a
re-queried DB layer and without introducing a self-only field.

Same-author tie-breaker: the strict-greater `created_at` rule
in the fold (see Fold behavior) means an event with a
`created_at` equal to the stored one is dropped — first decode
wins. Cross-author cases don't contend at all because the map
is keyed by author. No "last decoded wins" semantics anywhere.

## Profile pictures: deferred for Mode B

The Mode B editor has **no picture field**. Mode B in-channel
profiles are name-only. The reasoning:

- **Data URLs** (inline base64) are technically valid in NIP-01
  `picture` but bloat events above relay payload caps
  (~64 KiB), aren't rendered by mainstream nostr clients, and
  carry an "implementation smell" feel in the ecosystem.
- **External hosting** (Blossom, NIP-96, etc.) is the
  conventional answer but requires real plumbing: auth events
  (kind 24242), multipart upload, error handling, server
  selection, plus a crop UI on the picker side. That's a plan
  of its own, deferred.
- The "switch to Mode A" path remains the simple escape hatch:
  if a user wants a picture, they can publish a kind 0 with one
  via their normal nostr client and import that nsec here.

Mode A's read-only view continues to display the picture from
the cached public profile if present — that's just an HTTP URL
from the upstream kind 0, no app work involved.

Avatar fallback in the chat UI: when a `NostrProfile.picture`
is `None` (Mode B users, plus Mode A users whose kind 0 has no
picture), the existing `NostrAvatar` widget renders the
pubkey-derived placeholder it already uses today. No new
fallback code.

## Profile Settings UI (mode-aware)

The Profile Settings page renders differently per mode. The
existing identity-management buttons (`Export nsec`, `Import
different nsec`, `Generate new random identity`, `Remove`) sit
below the mode-specific section in both cases.

### Mode A — read-only public profile

- Avatar + name display, drawn from `cached_public_profile`.
- Subtitle: *"Your profile is managed via your other nostr
  clients. Edits made there propagate to your groups."*
- Refresh button: re-fetches the public kind 0 (uses the runtime
  5s timeout) and updates `cached_public_profile`.
- No editor — name and picture fields are not shown.

### Mode B — read-only name display

- Avatar (pubkey-derived fallback — Mode B doesn't have a
  picture) + the name set at generation time, displayed as
  read-only text. Subtitle: *"To change your name, generate
  a new identity below."*
- No editor, no Save button, no publish-from-here path. New
  channels you join pick up the name automatically via the
  connect-time publish trigger.

### Public kind 0 is NEVER published from the app

In Mode B, peers learn the user's profile from in-channel kind 0
events. There is no app surface — button, hidden setting,
nothing — that would publish a public (un-encrypted) kind 0 to
the wider network from an app-generated nsec. Doing so would
publicly link the in-channel identity to network visibility,
which is the opposite of what Mode B is for. If the user wants
that, they can switch to Mode A by setting up a profile in
another nostr client and importing the matching nsec.

## Architectural rule: profile logic lives at the runner

Publishing, dedup, and folding of in-channel kind 0 events are
all properties of the **encrypted channel** abstraction —
`ChannelRunner` / `ChannelRunnerHandle`. They MUST NOT be
duplicated in `signing/mod.rs::ChannelHandle` or
`keygen/lobby.rs::LobbyHandle`. Both wrappers expose the same
capability by reaching into their underlying
`ChannelRunnerHandle::publish_profile(...)`.

The wrapper layers do only what's specific to them:

- **Signing layer**: translates the runner's
  `MemberProfileUpdated` event into
  `ChannelEvent::MemberProfileUpdated` so the chat sink carries
  it.
- **Lobby layer**: translates the runner's
  `MemberProfileUpdated` event into a `LobbyEvent` /
  `LobbyState` change so the lobby sink carries it.

Each wrapper's connect path (chat `connect_to_channel`, lobby
`create_remote_lobby`, lobby `join_remote_lobby`) calls
`runner_handle.publish_profile(...)` after constructing the
handle if `NostrClient.local_publish` has credentials. That's
the only publish call site in the app — no registry of
handles, no `publish_everywhere` method, no save-triggered
re-publish.

## Files

### Rust

#### Storage

- `frostsnapp/rust/src/nostr_settings_state.rs` (existing):
  add `UserIdentity` sum type with `#[derive(Serialize,
  Deserialize)]`, `SetIdentity { identity, nsec }` mutation,
  load/save of the JSON-serialized identity under key
  `'identity'` in the existing `nostr_settings` kv table.

#### Runner — the only place publish/dedup/fold logic exists

- `frostsnap_nostr/src/channel_runner.rs`:
  - `NostrProfile` gains `derive(PartialEq, Eq, Serialize,
    Deserialize)`.
  - `MemberSlot { profile, source: ProfileSource,
    inchannel_created_at: Option<u64> }` and `ProfileSource {
    InChannel, External }`.
  - `ChannelState.members: HashMap<PublicKey,
    Option<MemberSlot>>`.
  - `process_inner_event` recognizes `Kind::Metadata`: parses
    `nostr_sdk::Metadata` → `NostrProfile`, applies the
    strict-greater `created_at` fold rule, emits
    `ChannelRunnerEvent::MemberProfileUpdated`.
  - The external-fetch path (`spawn_profile_fetch` + the
    `profile_rx` receive arm) constructs
    `MemberSlot { source: External, inchannel_created_at: None }`
    and respects the in-channel-wins precedence.
  - **`ChannelRunnerEvent::MemberProfileUpdated { pubkey,
    profile }`** — runner event, consumed by both signing and
    lobby wrappers.
  - **`ChannelRunnerHandle::publish_profile(profile, keys) ->
    Result<Option<EventId>>`** — the ONE implementation.
    Populates `profile.pubkey = Some(keys.public_key())`,
    derives the dedup target via `member_profile(self_pubkey)`,
    no-ops on match, otherwise builds + signs +
    `dispatch_prepared`s the inner metadata event.
  - `members()` projects slots back to
    `HashMap<PublicKey, Option<NostrProfile>>` (compat with the
    existing `GroupMetadata` consumers).
  - `member_profile(pubkey)` single-author accessor.

#### Signing wrapper (chat)

- `frostsnap_nostr/src/signing/events.rs`: add
  `ChannelEvent::MemberProfileUpdated { pubkey, profile }`.
- `frostsnap_nostr/src/signing/mod.rs`:
  - Map runner's `MemberProfileUpdated` → sink as
    `ChannelEvent::MemberProfileUpdated`.
  - **Delete `ChannelHandle::publish_profile`** — callers go
    through the wrapper's `runner_handle.publish_profile(...)`
    instead. (Optionally re-export an inline forwarder if
    callsites want symmetry, but no duplicated logic.)

#### Lobby wrapper (keygen)

- `frostsnap_nostr/src/keygen/lobby.rs`:
  - In the `LobbyClient::run` event loop, handle
    `ChannelRunnerEvent::MemberProfileUpdated { pubkey, profile
    }` by **upserting** the participant slot before assigning
    the profile:
    ```rust
    let entry = lobby
        .participants
        .entry(pubkey)
        .or_insert_with(ParticipantInfo::default);
    entry.profile = Some(profile);
    ```
    Profile events are independent of `Presence` / `Register`
    on the wire and can arrive first — without the upsert the
    update would be dropped. Devices, status, and the existing
    `upsert_joining` invariants must be preserved (use
    `entry.or_insert_with(ParticipantInfo::default)` or extend
    `upsert_joining` to take a callback that mutates the entry,
    whichever fits cleanest). Then emit
    `LobbyEvent::LobbyChanged(state.clone())`.
  - `ParticipantInfo` gains a `pub profile: Option<NostrProfile>`
    field, defaulting to `None`; populated from the
    `MemberProfileUpdated` fold (in-channel kind 0) and from
    the runner's existing external-fetch path
    (`spawn_profile_fetch`), which also routes through the same
    `MemberProfileUpdated` event after this revision — so the
    lobby's event loop has one place to update profile state.
  - `LobbyHandle` exposes its inner `runner_handle` (or a thin
    `LobbyHandle::publish_profile` forwarder that delegates to
    `runner_handle.publish_profile`) so the FRB-side
    `RemoteLobbyHandle` can wire it.

#### FRB / app boundary

- `frostsnapp/rust/src/api/nostr/mod.rs`:
  - `NostrSettings`: identity-mode methods —
    `current_identity()`, `generate_new_identity(name)`
    (now takes a name, since the name is part of identity
    creation), `set_imported_identity(nsec,
    cached_public_profile)`, `clear_identity()`.
  - **Remove `set_generated_name`** — name is set at
    `generate_new_identity` time and never edited afterwards.
  - **Remove `publish_profile_in_all_channels` /
    `publish_profile_everywhere`** — no cross-channel iteration.
  - `NostrClient.local_publish: Mutex<Option<(NostrProfile,
    String)>>` and `set_local_publish_credentials` stay —
    Dart writes them after every identity mutation
    (`generate_new_identity`, `set_imported_identity`,
    `clear_identity`); each connect path reads them.
  - `NostrClient::fetch_profile_for_import(nsec)` — Mode A
    import gate (cache → 30s relay fetch).
  - `connect_to_channel` (chat): after inserting the
    `ChannelHandle` into `NostrClient.channels`, runs the
    auto-publish-on-connect tokio task (see "Publish trigger"
    above).
- `frostsnapp/rust/src/api/nostr/remote_keygen.rs`:
  - `create_remote_lobby` and `join_remote_lobby`: after the
    `LobbyHandle` is built, run the SAME auto-publish-on-connect
    tokio task, reading `local_publish` from the surrounding
    `NostrClient`. Factor a tiny shared helper on `NostrClient`
    if you don't want to inline it three times — but it really
    is three lines of code per call site, so duplication is
    fine.
  - No registry, no guards. The lobby's runner lives inside the
    `LobbyHandle`'s tokio task as today; dropping
    `RemoteLobbyHandle` tears down the lobby naturally.
- `_ChannelEvent` mirror gains `MemberProfileUpdated`. New
  `_UserIdentity` mirror.
- `_LobbyState` mirror gains the new `profile` field on
  participants.

### Dart

- `frostsnapp/lib/nostr_chat/profile_settings_page.dart`:
  mode-aware view. Mode A: read-only public profile card +
  Refresh button. Mode B: read-only name display (no editor,
  no Save button). Identity-management buttons in the advanced
  section. **No public-kind-0 publish button.** Delete the
  existing `_GeneratedModeSection` editor widget + its
  `setGeneratedName` / publish-on-save wiring entirely.
- `frostsnapp/lib/nostr_chat/setup_dialog.dart`: Generate flow
  asks for a name up front; Import flow runs the
  `fetchProfileForImport` gate and surfaces rejection inline.
- `frostsnapp/lib/nostr_chat/nostr_state.dart`:
  `NostrContext.refreshPublishCredentials(client)` keeps the
  client's `local_publish` snapshot in sync; called after every
  identity mutation and once on first client init.
- `frostsnapp/lib/nostr_chat/chat_page.dart`: handles
  `ChannelEvent_MemberProfileUpdated` (already wired) +
  switches to using the shared `NostrContext.nostrClient`.
- `frostsnapp/lib/org_keygen_page.dart` — **the load-bearing
  change for this revision:**
  - Stop rendering the custom circle + hex pubkey for peers.
    Each `_ParticipantRow` renders a `NostrAvatar` and a name
    derived from a profile lookup:
    - For self: read from `UserIdentity` so the name shows even
      before any publish round-trips.
    - For peers: read from a lobby-scoped profile cache (see
      below). Fall back to a short pubkey if no profile is in
      the cache yet.
  - The lobby's Dart layer subscribes to the lobby state stream
    and pulls profile data out of the participants list (new
    `profile` field on `ParticipantInfo`). External fetching is
    NOT triggered from Dart — the runner's existing
    `spawn_profile_fetch` already fires on first observation of
    every author and produces a `MemberProfileUpdated` event,
    which lands in the lobby's event loop and updates the same
    participant slot. The Dart layer just reads.
  - Replace the leading "first letter / star" circle with
    `NostrAvatar` sized for the row; keep the initiator-star
    overlay as a Stack child if visually wanted.

### FRB

- `just gen` after the API additions.

## Out of scope (MVP)

- **In-app renaming of a Mode B identity.** Name is fixed at
  identity-generation time. Renaming requires solving
  invalidation across cached profiles, historical channel
  publications, and the UX of "your friends won't see the new
  name until they reconnect" — a future plan.
- **Profile pictures for Mode B.** Mode B is name-only;
  users wanting a picture switch to Mode A. A future plan can
  add a Blossom upload + crop pipeline.
- Editing fields beyond `name` for Mode B (display_name, about,
  banner, nip05, website are round-tripped in fetched profiles
  but never set).
- Per-channel different profiles (one identity, same name
  everywhere).
- Profile presence indicators / online status (NIP-37 etc).
- Profile event signing key separation (uses the same key the
  user already uses for chat / signing-event publication).

## Verification

- `cargo check --workspace` clean.
- `flutter analyze lib` clean.
- `just gen` clean.
- Manual:
  1. **Generate Mode B identity with name.** In the welcome
     dialog (or via the advanced page's "Generate new random
     identity"), the Generate dialog requires a name input.
     Enter "Lloyd" → confirm. Profile Settings shows Mode B
     with "Lloyd" as a read-only label and a subtitle directing
     to "generate a new identity" to change it.
  2. **No editor surface in Mode B.** Profile Settings has no
     name TextField, no Save button. The only ways to change
     the name are: regenerate identity or import a different
     nsec.
  3. **Mode A import gate.** Import an nsec whose public kind
     0 exists on a default relay → accepted, Mode A view shows
     the public profile card. Import an nsec with no public
     kind 0 → rejection dialog; previous identity stays.
  4. **Mode A no in-channel publish.** With a Mode A identity,
     join a channel → confirm no encrypted kind 0 from the
     local user lands in the channel (peers see the user via
     the public-kind-0 fetch path only).
  5. **Lobby publish (Mode B), self → peer.** Host instance is
     Mode B with name "Lloyd"; host creates a wallet → lobby
     opens. Joiner instance opens the same lobby. Joiner's
     `_ParticipantRow` for the host shows "Lloyd" + the
     pubkey-derived avatar within seconds of opening the
     lobby. Joiner → host symmetric.
  6. **Lobby Mode A profile pic.** Host is Mode A with a
     public kind 0 carrying a picture URL → opens a lobby.
     Joiner sees the host's avatar rendered from the picture
     URL (fetched via the runner's external-fetch path).
  7. **Chat publish (Mode B).** Open a chat channel after
     keygen completes. The local user's name lands in peers'
     chat bubbles via the chat-side auto-publish-on-connect.
  8. **Chat bubble fallback.** Mode B users with no picture
     render the existing pubkey-derived `NostrAvatar` fallback;
     no regression in avatar UI.
  9. **Restart duplicate-publish.** Restart the app and rejoin
     the same channel → at most one extra kind 0 lands in the
     channel (documented MVP race), no UX-visible change.

## Risk notes

- **Replay vs publish race**: the dedup check uses the local
  cache; if our own earlier publish hasn't been folded into
  `state.members` yet when the connect trigger fires, we
  re-publish. Net effect: at most one duplicate event per
  channel per app restart. Relays dedupe by event id;
  last-`created_at`-wins downstream keeps UX correct.
- **No mid-session rename.** Users who want to change their
  name must regenerate their identity, which costs them their
  nsec. Documented limitation, addressed in a future plan.
