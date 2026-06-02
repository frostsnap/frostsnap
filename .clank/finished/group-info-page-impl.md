# Group Info page implementation

Implementation plan for the design at
`.clank/finished/group-info-page-redesign.md`. The user's
constraints:

- Existing `KeysSettings` is bad — do NOT route through it. Use
  `RecoveryFlowWithDiscovery` directly via the existing
  `WalletAddColumn.showAddKeyDialog` helper.
- Design can be more boring — skip the alpha-tinted self-row, the
  "Tap to manage your devices" hint, etc. Use plain Material 3
  tiles. The badge `(you)` and the trailing pencil-icon are enough
  to communicate "this is yours."

## Files

- `frostsnapp/lib/nostr_chat/group_info_page.dart` — full body
  rewrite (keep the `GroupInfoPage` class; just change `build`)
- `frostsnapp/lib/nostr_chat/member_detail_sheet.dart` — extend
  with a self-only "DEVICES" + actions section

No Rust changes.

## Layout (concrete)

```dart
ListView(children: [
  // Band 0: header
  GroupHeader(
    walletName: walletName,
    memberCount: members.length,
    threshold: threshold,         // 0 if unknown
    totalShares: totalShares,
  ),

  // Band 1: group actions (rounded grouped tiles)
  _SectionLabel('WALLET'),
  _GroupedTiles([
    _Tile(icon: Icons.shield_rounded,
          title: 'Back up my keys',
          subtitle: 'Save your shares to paper or steel',
          onTap: _openBackupChecklist),
    _Tile(icon: Icons.pin_drop_rounded,
          title: 'Check address',
          subtitle: 'Verify an address is in this wallet',
          onTap: _openCheckAddress),
    _Tile(icon: Icons.code_rounded,
          title: 'Show descriptor',
          subtitle: 'Miniscript descriptor for export',
          onTap: _showDescriptor),
  ]),

  // Band 2: members
  _SectionLabel('MEMBERS'),
  _GroupedTiles([
    // You first, regardless of share-index order
    _MemberTile(
      pubkey: myPubkey,
      isYou: true,
      shareIndices: _mySharesFromLocal(context),
      trailing: IconButton(
        icon: Icon(Icons.edit_rounded),
        onPressed: () => _showMemberDetail(context, myPubkey),
      ),
      onTap: () => _showMemberDetail(context, myPubkey),
    ),
    // Others sorted by lowest share index
    for (final m in _otherMembersSorted)
      _MemberTile(
        pubkey: m,
        isYou: false,
        shareIndices: _shareIndicesFor(m),
        trailing: Icon(Icons.chevron_right),
        onTap: () => _showMemberDetail(context, m),
      ),
    // "+ Invite" as the last tile in the members band
    _Tile(icon: Icons.person_add_rounded,
          title: 'Invite someone',
          subtitle: 'Copy invite link',
          onTap: _copyInviteLink),
  ]),

  SizedBox(height: 32),  // larger gap before danger zone

  // Band 3: danger
  _DangerTile(
    icon: Icons.exit_to_app_rounded,
    title: 'Leave remote wallet',
    subtitle: 'Switch back to local coordination',
    onTap: _confirmLeaveRemote,
  ),

  SizedBox(height: 24),
])
```

### `_otherMembersSorted` — Q1 answer

Sort by lowest share index (channel state's `participantShares`).
Members with no shares in `_participantShares` (e.g., older wallets
where channel state isn't populated) fall back to alphabetical at
the end. Stable sort.

### `_GroupedTiles` helper

Reuses the `tileShapeTop` / `tileShape` / `tileShapeEnd` pattern
from `wallet_more.dart` so consecutive tiles render as one rounded
card with internal dividers. Implementation: take a list of widgets,
apply the appropriate shape to each based on position.

### `_SectionLabel`

Small uppercase label above each band. Existing code in
`group_info_page.dart` already has this exact pattern — extract to
a helper.

## Self-only member detail sheet additions

`MemberDetailSheet` currently shows: avatar, name, share chips,
npub, NIP-05, about, "Open in Nostr Client" button.

When `isMe` is true, add (after the existing content):

```dart
Divider(height: 32),

Align(
  alignment: Alignment.centerLeft,
  child: Text('YOUR DEVICES',
    style: theme.textTheme.labelSmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    ),
  ),
),
SizedBox(height: 8),

// One tile per local device, showing device name + share #
for (final entry in deviceShareEntries)
  ListTile(
    leading: Icon(Icons.usb_rounded),
    title: Text(entry.deviceName),
    subtitle: Text('Share #${entry.shareIndex}'),
    visualDensity: VisualDensity.compact,
  ),

SizedBox(height: 16),

SizedBox(
  width: double.infinity,
  child: OutlinedButton.icon(
    onPressed: () => _restoreFromBackup(context),
    icon: Icon(Icons.restore_rounded),
    label: Text('Restore a share from backup'),
  ),
),
```

### `deviceShareEntries`

Pulled from the local access structure, same source as
`_mySharesFromLocal`:

```dart
final entries = <({String deviceName, int shareIndex})>[];
for (final deviceId in accessStruct.devices()) {
  final idx = accessStruct.getDeviceShortShareIndex(deviceId: deviceId);
  if (idx == null) continue;
  entries.add((
    deviceName: coord.getDeviceName(id: deviceId) ?? '<unnamed>',
    shareIndex: idx,
  ));
}
entries.sort((a, b) => a.shareIndex.compareTo(b.shareIndex));
```

This data is already being computed in
`group_info_page.dart::_showMemberDetail`'s `deviceNames` map —
restructure as a sorted list of records and pass through.

### `_restoreFromBackup`

```dart
void _restoreFromBackup(BuildContext context) {
  Navigator.pop(context);  // close the detail sheet first
  WalletAddColumn.showAddKeyDialog(
    context,
    AccessStructureRef(
      keyId: walletCtx.keyId,
      accessStructureId: accessStructureId,
    ),
  );
}
```

`showAddKeyDialog` already exists at `wallet_add.dart:255` and
opens `RecoveryFlowWithDiscovery` with `RecoveryContext.addingToWallet(...)`.
No new flow needed.

(Out of scope: no separate "add a brand new device" action — the
recovery flow IS the add-device flow for an existing access
structure. Confirmed via inspection of `RecoveryContext.addingToWallet`.)

## "Back up my keys" — Q3 answer

The existing `BackupChecklist` (`wallet_more.dart:155`) takes an
`AccessStructure` and shows all devices in it. Since the local
access structure on a remote wallet ALREADY only contains local
devices (post-`finalize_remote_keygen`), passing it directly will
naturally show only this user's shares. No filtering needed.

```dart
void _openBackupChecklist() async {
  final accessStruct = coord.getAccessStructure(
    asRef: AccessStructureRef(
      keyId: walletCtx.keyId,
      accessStructureId: accessStructureId,
    ),
  );
  if (accessStruct == null) return;
  await MaybeFullscreenDialog.show(
    context: context,
    child: walletCtx.wrap(
      BackupChecklist(
        accessStructure: accessStruct,
        showAppBar: true,
      ),
    ),
  );
}
```

Use the page's own `accessStructureId` (already a field on
`GroupInfoPage`) — `accessStructures()[0]` would target the wrong
access structure if the wallet has more than one.

## "Check address" + "Show descriptor"

Pull directly from `wallet_more.dart`:

```dart
void _openCheckAddress() async {
  await MaybeFullscreenDialog.show(
    context: context,
    child: walletCtx.wrap(CheckAddressPage()),
  );
}

void _showDescriptor() {
  showExportWalletDialog(
    context,
    walletCtx.network.descriptorForKey(
      masterAppkey: walletCtx.wallet.masterAppkey,
    ),
  );
}
```

Note: the existing `</> ` icon in the app bar (top-right) opens
`showExportWalletDialog` — REMOVE it from the app bar now that
"Show descriptor" is a band-1 tile. Avoids duplicate affordance.

## Self-highlighting (toned down per "more boring" constraint)

The design plan recommended `primaryContainer` tinted background
for the self row. Per the user's "more boring" note, skip the tint.
Differentiators left:

- `(you)` badge in primary color (existing)
- Trailing `IconButton(Icons.edit_rounded)` for self vs
  `Icon(Icons.chevron_right)` for others
- Position: always first in the members band

That's enough.

## What stays the same

- `MyProfileBuilder` widget wrapping the self tile
- `ProfileBuilder` for other members
- The "Manage your Nostr profile" → `ProfileSettingsPage` link
  inside the detail sheet (already there)
- Channel state listening + `_participantShares` updates

## Verification

- `flutter analyze lib/nostr_chat/` clean
- Manual: open Group Info in a 1-of-1 (just you) wallet → see your
  row with share chip + Manage icon, no "MEMBERS" empty state weird
- Manual: open in a multi-member remote wallet → see all members,
  yours highlighted by badge + icon, sorted by lowest share index
- Manual: tap your row → detail sheet shows YOUR DEVICES + "Restore
  a share from backup" button
- Manual: tap Restore → recovery flow opens with `addingToWallet`
  context targeting this access structure
- Manual: descriptor opens from band-1 tile, app-bar `</>` icon is
  gone
- Manual: Leave remote wallet is at the bottom, visually separated

## Out of scope

- The redesigned key management screen the user mentioned briefly —
  this plan does NOT touch `wallet_more.dart` or `KeysSettings`.
  Those are for the local-wallet path and stay broken-as-is.
- "Add a brand new device that has no share yet" — not a real flow
  in FROST. Excluded.
- Removing the wallet → handled by `_confirmLeaveRemote` (existing,
  unchanged in this plan beyond reposition).
