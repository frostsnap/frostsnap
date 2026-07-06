# recovered_wallet_goes_remote
# A remotely-recovered wallet comes up as a remote wallet

User-confirmed gap: after a remote recovery the wallet renders in
the local shell. The chat-first remote shell is gated on
`coordination_ui_enabled` for the access structure
(`wallet.dart` → `watchCoordinationUi`), and only the post-keygen
flow ever sets it: `org_keygen_page.dart`'s "Setting up signing
channel…" step runs `connectMaybeCreateChannel` → waits for
`ChannelEvent_ChannelState` confirmation (retry loop, persistent
progress dialog) → `setCoordinationUiEnabled(asRef, true)`. The
recovery finish (`wallet_add.dart` `showRemoteRecoveryDialog` and
the joined-via-link path) goes persist → unplug prompt →
`openNewlyCreatedWallet` — no channel, no flag.

Recovery can rejoin the ORIGINAL wallet channel: the coordination
channel secret is `ChannelSecret::from_access_structure_id`
(deterministic), and a recovered access structure has the original
id. If the channel still exists on the relays, joining picks up the
original creation event (and history). If it doesn't,
`connectMaybeCreateChannel` creates it — and the creation event's
`ChannelInitData.participants` (the pubkey → share-index
assignment, the channel-as-source-of-truth for the Members page)
MUST be populated with the recovery-derived assignment, not left
empty.

## Task 1 — extract the channel-setup helper

Extract org_keygen_page's post-keygen block (progress dialog +
connect → confirm → retry → `setCoordinationUiEnabled`) into a
shared helper, e.g. `lib/nostr_chat/channel_setup.dart`:

```dart
Future<void> setupCoordinationChannel(
  BuildContext context, {
  required AccessStructureRef asRef,
  required List<ChannelParticipant> participants,
})
```

Keygen adopts the helper with zero behavior change (same dialog
copy, same listen-then-start subscription order, same retry loop).

## Task 2 — recovery-side share assignment (Rust)

The Dart snapshot can't derive share indices (`ShareImage` is
opaque), so expose it from the transport:
`RemoteRecoveryLobbyHandle::channel_participants() ->
Vec<ChannelParticipant>` — derived from the finished recovery:
`finished.share_refs` → `ObservedShare` → (author pubkey,
`share_image.index`), grouped by author. Winning shares only — the
assignment must describe the recovered access structure, not every
posted share. Lives in frostsnap_nostr (lobby state method) + thin
FRB wrapper; `just gen`.

## Task 3 — wire into both recovery finish paths

In `wallet_add.dart`, both completion sites (leader
`showRemoteRecoveryDialog` and the joined-via-link dialog) run,
after persist and before `openNewlyCreatedWallet`:
`setupCoordinationChannel(asRef, participants:
handle.channelParticipants())`. Both leader and joiners run it —
every participant persists the wallet locally and each needs the
channel + flag. The unplug prompt stays where it is.

Accepted semantics (note, not new work): when the original channel
still exists, join wins and the ORIGINAL creation metadata stands —
recovery does not rewrite an existing channel's share assignment;
maybe-create only stamps the recovery-derived assignment on a
freshly created channel.

## Tests

- Rust: unit test on the lobby state for `channel_participants`
  derivation (multi-share participant, non-winning posted share
  excluded); assert in `recovery_live` that the finished handle
  yields the expected pubkey → index assignment for all three
  participants.
- Dart: the helper is dialog + live-FFI heavy; the derivation seam
  is covered in Rust. Add a widget test only if it doesn't require
  standing up a live NostrClient; otherwise the keygen path's
  existing coverage carries the helper.

## Acceptance

- `cargo test -p frostsnap_nostr` + `flutter analyze` + Dart suites
  green; keygen flow behavior unchanged.
- Manual (user): recover a wallet remotely → it opens in the remote
  (chat-first) shell; Members page shows the recovered share
  assignment; recovering when the original channel still exists
  drops you into the existing channel.
