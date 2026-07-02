# channel_profiles_single_source
# Profiles live in the channel runner; app folds can't touch them

A user walkthrough of the remote recovery lobby found joiners seeing
pubkeys instead of names — including their own. Root cause
(diagnosed, deliberately left unfixed by [[redesign-recovery-ui]]):
the recovery fold gates state creation on the channel metadata and
drops `MemberProfileUpdated` events that arrive pre-gate
(`frostsnap_nostr/src/recovery/lobby.rs`, the
`if let Some(state) = &mut fold` arm with no else). Two real paths
hit the gate window for joiners: their own identity profile is
dispatched on connect before background sync delivers the creation
event, and the leader's auto-published profile ties the creation
event's `created_at` second so cache replay can order it first.

The deeper problem is architectural: the channel runner ALREADY owns
an authoritative profile fold — `ChannelState.members`, with the
precedence rules (in-channel kind-0 beats external fetch,
strict-greater `created_at` wins on replay) — but each consumer
re-folds `MemberProfileUpdated` events into its own copy
(`RecoveryParticipantInfo.profile`, keygen's
`ParticipantInfo.profile`). Duplicated state + per-app folds = every
app gets a fresh chance to gate, drop, or reorder profiles. Recovery
took that chance; keygen merely happens not to have a gate yet.

## Direction (user-stated)

The channel runner maintains the profile fold independent of the
app-state fold. The API exposes the underlying channel members as
directly as possible so application code is never tempted to
intervene. App layers may FILTER what they show (e.g. only render
lobby participants), but they don't MANAGE profile state. The most
straightforward implementation: just pass the events through.

## Invariants

1. **One profile fold, owned by the runner.** `ChannelState.members`
   is the only place profile precedence is decided. No consumer
   holds a `profile` field in its own fold state.
2. **Profile events pass through app wrappers untouched.** The
   lobby wrapper tasks forward `MemberProfileUpdated` verbatim on
   their sink enums — no gate, no buffering, no upsert-into-fold.
   A lobby bug can lose a share post; it structurally cannot lose a
   name.
3. **Snapshot + stream.** Consumers late to the party don't depend
   on having seen every pass-through event:
   `ChannelRunnerHandle::member_profiles()` returns the current
   `HashMap<PublicKey, NostrProfile>` snapshot (read from
   `state_arc()`), and the pass-through stream carries changes.
   The FRB layer seeds its broadcast from the snapshot and updates
   it from the stream, so ordering races against the metadata gate
   are structurally irrelevant.
4. **UI joins at render time.** Participant lists come from the
   lobby fold (pubkeys, joined-at, shares, left); names/avatars
   come from the profile broadcast keyed by pubkey. This keeps the
   earlier "names come from the fold only" rule — the runner's
   member map IS the channel-derived truth, owned by the right
   layer; no settings-name or side-channel fallbacks return.

## Tasks

1. **frostsnap_nostr — runner surface.**
   - Add `ChannelRunnerHandle::member_profiles() ->
     HashMap<PublicKey, NostrProfile>` (snapshot of
     `state.members`, flattening the `MemberSlot`).
   - No changes to the fold or precedence logic — it's already
     correct.

2. **frostsnap_nostr — recovery lobby.**
   - Delete `profile` from `RecoveryParticipantInfo`.
   - Replace the `MemberProfileUpdated` fold arm with a verbatim
     pass-through: add
     `RecoveryLobbyEvent::MemberProfileUpdated { pubkey, profile }`
     and emit it unconditionally (before or after the metadata
     gate — there is no gate for profiles anymore).
   - Delete the pre-gate profile handling question entirely; the
     `upsert_participant(_, _, 0)` call for profile-only authors
     goes away (profiles no longer create participants — presence
     and share posts do).

3. **frostsnap_nostr — keygen lobby.** Same surgery:
   `ParticipantInfo.profile` deleted, `LobbyEvent` gains the
   pass-through variant, fold arm removed. Chat's channel handle
   already forwards profile events (`ChannelEvent.memberProfileUpdated`)
   — unchanged, it was already the right shape.

4. **FRB wrappers (frostsnapp/rust).**
   - Recovery bridge: maintain a
     `BehaviorBroadcast<Vec<MemberProfile>>` (FRB-friendly list of
     pubkey + profile) seeded from
     `runner_handle.member_profiles()` and updated from the
     pass-through events; expose `sub_member_profiles()` on
     `RemoteRecoveryLobbyHandle`.
   - Keygen bridge (`RemoteLobbyHandle`): same.
   - `RecoveryParticipantInfo` / keygen `ParticipantInfo` mirrors
     lose their `profile` field.

5. **Dart.**
   - Lobby pages subscribe to `subMemberProfiles()` alongside the
     state broadcast and hand the map to the pure views
     (`RecoveryLobbyView` gains a `profiles:
     Map<PublicKey, NostrProfile>` param; same for keygen's
     participant rows).
   - Row rendering unchanged otherwise: name from the map, "You" /
     short-pubkey fallbacks stay.

6. **Regression test (frostsnap_nostr).** Extend
   `tests/recovery_live.rs`: after the joiners connect, assert each
   participant's profile surface (snapshot + received pass-through
   events) contains BOTH its own profile and the leader's — the
   exact case the walkthrough caught. This is deterministic now:
   there is no gate to race.

7. **Flutter tests.** Adapt `recovery_lobby_view_test.dart` to the
   `profiles` param (name-resolution cases now build the map
   instead of `RecoveryParticipantInfo.profile`).

## Deliberately NOT done

- No changes to runner profile precedence or fetch behavior.
- No Dart-side profile caching layer changes (`NostrContext`
  keeps its chat-oriented cache; the lobbies don't use it).
- No revival of settings-name fallbacks.

## Acceptance

- `cargo test -p frostsnap_nostr` green including the new
  regression case.
- `flutter analyze` + recovery/keygen widget tests green.
- Manual: the walkthrough scenario — leader creates, joiner joins —
  now shows both names on both sides, including the joiner's own,
  without the joiner posting anything.
