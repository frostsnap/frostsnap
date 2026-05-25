# Members Page Redesign

## Goal

Add key-share ownership display to the Group Info / Members page
(`lib/nostr_chat/group_info_page.dart`).

The wallet management surface (descriptor, coord toggle, leave) is already
implemented on this page. This plan covers the remaining work: showing
**who holds which key shares** per member.

## Architecture: channel as state store

The participant→share mapping is NOT stored locally. It lives in the
nostr signing channel as a fold over channel events:

1. **Channel creation message** (this plan): after keygen, the channel
   init data includes the resolved participant mapping —
   `Vec<ParticipantShares>` where each entry is `{ pubkey, share_indices }`.
   All participants derive the same deterministic mapping from
   `ResolvedKeygen`, so the creation message is identical regardless of
   who creates the channel. DeviceIds are NOT included — only the nostr
   pubkey → share index association matters.

2. **Signature-based discovery** (follow-up plan): when a nostr profile
   successfully signs (valid partial sig received), that key share is
   associated with their profile. This handles the case where devices
   change hands or a participant regains access.

3. **Removal declarations** (follow-up plan): a channel message declaring
   "I no longer have access to key share #N". Observers fold this in.

For this plan, only (1) is implemented. The UI reads the initial mapping
from `ChannelInitData` and displays it. No local state, no persistence —
just read the channel.

## Data flow

### Rust side (already implemented)
- New struct in `frostsnap_nostr::channel`:
  ```rust
  pub struct ParticipantShares {
      pub pubkey: PublicKey,
      pub share_indices: Vec<ShareIndex>,
  }
  ```
- `ParticipantShares::from_device_lists(participants, devices_in_order)`
  builder cross-references pubkey→devices with positional share indices.
- `ChannelInitData` gains field `participants: Vec<ParticipantShares>`.

### Keygen → channel handover (Dart-driven)

The participant data flows through the Dart widget stack — no local
persistence, no SQL:

1. **Keygen completes** — `_LobbyAndKeygenPageState` listener fires when
   `kgState.finished != null` (`org_keygen_page.dart:920`).

2. **Data is available** — at this moment `_ctrl.lobbyState!.keygen!`
   is the `ResolvedKeygen` with `participants: List<SelectedParticipant>`.
   Each `SelectedParticipant` has `pubkey` + `devices: List<DeviceRegistration>`
   (which has `deviceId`).

3. **Build ParticipantShares** — Dart (or a Rust helper called from Dart)
   cross-references `resolved.participants` with `devices_in_order` to
   produce `Vec<ParticipantShares>`. This can call the existing
   `ParticipantShares::from_device_lists()` via FRB, or do it in Dart.

4. **Thread to channel creation** — the participant data is passed to
   `channel_connection_params()` (add an optional `participants`
   parameter) or stored on `ChannelConnectionParams` so that when
   `ChannelClient` publishes the creation event, participants are
   baked in.

5. **Subsequent connections** — `channel_connection_params()` passes
   `participants: vec![]`. `ChannelClient` finds the existing channel
   on the relay and reads the creation event (which has full
   participants from step 4). The empty vec is a no-op — the relay is
   authoritative.

### Dart side (GroupInfoPage)
- The page receives the participant mapping from the channel init data
  (threaded through `ChatPageBody` → `GroupInfoPage` or via a shared
  context/provider).
- Member rows show share badges (e.g., chips with "Key #1", "Key #3").
- The header area shows threshold (e.g., "2-of-5 required to sign").
- Tapping a member opens the detail sheet showing their shares.

## UI: member list (main page)

- Each member row: avatar, display name, **share chips** (e.g., `#1 #3`)
- Your own entry: distinguished with "(you)" suffix or badge
- Threshold: displayed below the wallet name in the header
  (e.g., "2-of-5 threshold")
- Members with overlapping share numbers: the chip color/style makes
  the number visually prominent so duplicates are obvious at a glance

## UI: member detail (tap a profile)

- Shows which key shares this member holds with share indices
- For YOUR profile: also shows device names per share
- For OTHER profiles: read-only share list (no device names — you don't
  know their device names)

## Design constraints

- Must work within the existing Group Info page structure
- Should use the app's existing Material 3 theme
- Key share numbers must be **visually super obvious** — not buried
- Needs frontend-design review for chip/badge styling

## Out of scope

- Signature-based share discovery (follow-up plan)
- Removal declarations (follow-up plan)
- Mutating other members' shares
- Key rotation / share redistribution
- The local Keys page redesign (separate plan)
