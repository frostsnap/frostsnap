import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/address.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/nostr_chat/member_detail_sheet.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_add.dart';
import 'package:frostsnap/wallet_more.dart';

const _bodyMaxWidth = 580.0;

const _contentPadding = EdgeInsets.symmetric(horizontal: 16);
const _tileShapeTop = RoundedRectangleBorder(
  borderRadius: BorderRadius.vertical(
    top: Radius.circular(24),
    bottom: Radius.circular(4),
  ),
);
const _tileShapeMid = RoundedRectangleBorder(
  borderRadius: BorderRadius.all(Radius.circular(4)),
);
const _tileShapeEnd = RoundedRectangleBorder(
  borderRadius: BorderRadius.vertical(
    top: Radius.circular(4),
    bottom: Radius.circular(24),
  ),
);
const _tileShapeSingle = RoundedRectangleBorder(
  borderRadius: BorderRadius.all(Radius.circular(24)),
);

class GroupInfoPage extends StatelessWidget {
  final String walletName;
  final List<PublicKey> members;
  final AccessStructureId accessStructureId;
  final List<ChannelParticipant> participantShares;

  const GroupInfoPage({
    super.key,
    required this.walletName,
    required this.members,
    required this.accessStructureId,
    this.participantShares = const [],
  });

  AccessStructureRef _asRef(BuildContext context) => AccessStructureRef(
    keyId: WalletContext.of(context)!.keyId,
    accessStructureId: accessStructureId,
  );

  List<int> _shareIndicesFor(PublicKey pubkey) {
    final hex = pubkey.toHex();
    for (final p in participantShares) {
      if (p.pubkey.toHex() == hex) return p.shareIndices.toList();
    }
    return const [];
  }

  List<int> _mySharesFromLocal(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return const [];
    final accessStruct = coord.getAccessStructure(asRef: _asRef(context));
    return accessStruct?.localShareIndices() ?? const [];
  }

  int _totalShares() =>
      participantShares.fold<int>(0, (sum, p) => sum + p.shareIndices.length);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final nostrState = NostrContext.of(context);
    final myPubkey = nostrState.myPubkey;
    final walletCtx = WalletContext.of(context);

    final frostKey = walletCtx != null
        ? coord.getFrostKey(keyId: walletCtx.keyId)
        : null;
    final threshold =
        frostKey
            ?.getAccessStructure(accessStructureId: accessStructureId)
            ?.threshold() ??
        0;
    final totalShares = _totalShares();

    final orderedMembers = _orderMembers(members, myPubkey);

    return Scaffold(
      appBar: AppBar(title: const Text('Group Info')),
      body: LayoutBuilder(
        builder: (context, constraints) {
          // Side padding centers content (max 580 wide) while letting
          // ListView span the full width — so the scrollbar sits at
          // the right edge of the page, not in the middle.
          final sidePad = ((constraints.maxWidth - _bodyMaxWidth) / 2).clamp(
            0.0,
            double.infinity,
          );
          return ListView(
            padding: EdgeInsets.symmetric(horizontal: sidePad),
            children: [
              const SizedBox(height: 24),
              Center(
                child: CircleAvatar(
                  radius: 48,
                  backgroundColor: theme.colorScheme.primaryContainer,
                  child: Icon(
                    Icons.group,
                    size: 48,
                    color: theme.colorScheme.onPrimaryContainer,
                  ),
                ),
              ),
              const SizedBox(height: 16),
              Center(
                child: Text(walletName, style: theme.textTheme.headlineSmall),
              ),
              Center(
                child: Text(
                  _headerSubtitle(members.length, threshold, totalShares),
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
              const SizedBox(height: 24),

              // Band 1: WALLET actions
              if (walletCtx != null) ...[
                _sectionLabel(theme, 'WALLET'),
                _walletActions(context, walletCtx),
                const SizedBox(height: 24),
              ],

              // Band 2: MEMBERS
              _sectionLabel(theme, 'MEMBERS'),
              _membersBand(context, theme, orderedMembers, myPubkey),

              const SizedBox(height: 32),

              // Band 3: Danger zone
              Padding(
                padding: _contentPadding,
                child: ListTile(
                  contentPadding: _contentPadding,
                  shape: _tileShapeSingle,
                  tileColor: theme.colorScheme.errorContainer.withValues(
                    alpha: 0.3,
                  ),
                  leading: Icon(
                    Icons.exit_to_app,
                    color: theme.colorScheme.error,
                  ),
                  title: Text(
                    'Leave remote wallet',
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                  subtitle: const Text('Switch back to local coordination'),
                  onTap: () => _confirmLeaveRemote(context),
                ),
              ),
              const SizedBox(height: 24),
            ],
          );
        },
      ),
    );
  }

  String _headerSubtitle(int memberCount, int threshold, int totalShares) {
    final memberText = '$memberCount member${memberCount != 1 ? 's' : ''}';
    if (threshold > 0 && totalShares > 0) {
      return '$memberText · $threshold-of-$totalShares';
    }
    return memberText;
  }

  Widget _sectionLabel(ThemeData theme, String text) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 32, vertical: 8),
    child: Text(
      text,
      style: theme.textTheme.labelSmall?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
        fontWeight: FontWeight.w600,
      ),
    ),
  );

  Widget _walletActions(BuildContext context, WalletContext walletCtx) {
    final theme = Theme.of(context);
    final tileColor = theme.colorScheme.surfaceContainerLow;
    final tiles = [
      ListTile(
        contentPadding: _contentPadding,
        tileColor: tileColor,
        shape: _tileShapeTop,
        leading: const Icon(Icons.shield_rounded),
        title: const Text('Back up my keys'),
        subtitle: const Text('Save your shares to paper or steel'),
        onTap: () => _openBackupChecklist(context),
      ),
      ListTile(
        contentPadding: _contentPadding,
        tileColor: tileColor,
        shape: _tileShapeMid,
        leading: const Icon(Icons.pin_drop_rounded),
        title: const Text('Check address'),
        subtitle: const Text('Verify an address is in this wallet'),
        onTap: () => _openCheckAddress(context, walletCtx),
      ),
      ListTile(
        contentPadding: _contentPadding,
        tileColor: tileColor,
        shape: _tileShapeEnd,
        leading: const Icon(Icons.code_rounded),
        title: const Text('Show descriptor'),
        subtitle: const Text('Miniscript descriptor for export'),
        onTap: () => _showDescriptor(context, walletCtx),
      ),
    ];
    return Padding(
      padding: _contentPadding,
      child: Column(spacing: 2, children: tiles),
    );
  }

  Widget _membersBand(
    BuildContext context,
    ThemeData theme,
    List<PublicKey> orderedMembers,
    PublicKey myPubkey,
  ) {
    final tileColor = theme.colorScheme.surfaceContainerLow;
    final tiles = <Widget>[];
    for (var i = 0; i < orderedMembers.length; i++) {
      final m = orderedMembers[i];
      final isMe = m == myPubkey;
      final shape = _shapeFor(i, orderedMembers.length + 1);
      tiles.add(
        _MemberTile(
          pubkey: m,
          isYou: isMe,
          shareIndices: isMe
              ? _mySharesFromLocal(context)
              : _shareIndicesFor(m),
          tileColor: tileColor,
          shape: shape,
          contentPadding: _contentPadding,
          onTap: () => _showMemberDetail(context, m),
        ),
      );
    }
    // "Invite someone" as the last tile in the members band
    tiles.add(
      ListTile(
        contentPadding: _contentPadding,
        tileColor: tileColor,
        shape: _tileShapeEnd,
        leading: const Icon(Icons.person_add_rounded),
        title: const Text('Invite someone'),
        subtitle: const Text('Copy invite link'),
        trailing: const Icon(Icons.copy_rounded),
        onTap: () => _copyInviteLink(context),
      ),
    );
    return Padding(
      padding: _contentPadding,
      child: Column(spacing: 2, children: tiles),
    );
  }

  ShapeBorder _shapeFor(int index, int total) {
    if (total == 1) return _tileShapeSingle;
    if (index == 0) return _tileShapeTop;
    if (index == total - 1) return _tileShapeEnd;
    return _tileShapeMid;
  }

  /// Self always first; others sorted by lowest known share index, then by
  /// pubkey hex for stability.
  List<PublicKey> _orderMembers(List<PublicKey> members, PublicKey myPubkey) {
    final mine = <PublicKey>[];
    final others = <PublicKey>[];
    for (final m in members) {
      if (m == myPubkey) {
        mine.add(m);
      } else {
        others.add(m);
      }
    }
    others.sort((a, b) {
      final aShares = _shareIndicesFor(a);
      final bShares = _shareIndicesFor(b);
      final aMin = aShares.isEmpty
          ? 1 << 30
          : aShares.reduce((x, y) => x < y ? x : y);
      final bMin = bShares.isEmpty
          ? 1 << 30
          : bShares.reduce((x, y) => x < y ? x : y);
      final cmp = aMin.compareTo(bMin);
      if (cmp != 0) return cmp;
      return a.toHex().compareTo(b.toHex());
    });
    return [...mine, ...others];
  }

  // ----- Action handlers -----

  void _openBackupChecklist(BuildContext context) async {
    final accessStruct = coord.getAccessStructure(asRef: _asRef(context));
    if (accessStruct == null) return;
    await MaybeFullscreenDialog.show(
      context: context,
      child: WalletContext.of(
        context,
      )!.wrap(BackupChecklist(accessStructure: accessStruct, showAppBar: true)),
    );
  }

  void _openCheckAddress(BuildContext context, WalletContext walletCtx) async {
    await MaybeFullscreenDialog.show(
      context: context,
      child: walletCtx.wrap(CheckAddressPage()),
    );
  }

  void _showDescriptor(BuildContext context, WalletContext walletCtx) {
    showExportWalletDialog(
      context,
      walletCtx.network.descriptorForKey(
        masterAppkey: walletCtx.wallet.masterAppkey,
      ),
    );
  }

  void _showMemberDetail(BuildContext context, PublicKey pubkey) {
    final profile = NostrContext.of(context).getProfile(pubkey);
    final nostr = NostrContext.of(context);
    final walletCtx = WalletContext.of(context);
    final isMe = pubkey == nostr.myPubkey;
    final canManageSelf = isMe && walletCtx != null;

    final keyIndices = isMe
        ? _mySharesFromLocal(context)
        : _shareIndicesFor(pubkey);

    final asRef = canManageSelf
        ? AccessStructureRef(
            keyId: walletCtx.keyId,
            accessStructureId: accessStructureId,
          )
        : null;
    final pageContext = context;

    final deviceKeys = <DeviceKeyEntry>[];
    if (canManageSelf) {
      final accessStruct = coord.getAccessStructure(asRef: asRef!);
      if (accessStruct != null) {
        for (final d in accessStruct.devices()) {
          final idx = accessStruct.getDeviceShortShareIndex(deviceId: d);
          if (idx == null) continue;
          deviceKeys.add(
            DeviceKeyEntry(
              deviceName: coord.getDeviceName(id: d) ?? '<unnamed>',
              keyIndex: idx,
            ),
          );
        }
        deviceKeys.sort((a, b) => a.keyIndex.compareTo(b.keyIndex));
      }
    }

    // Only show a title when we have a real Nostr display name —
    // the shortened-npub fallback looks ugly truncated in the bar.
    final hasName =
        (profile?.displayName?.isNotEmpty ?? false) ||
        (profile?.name?.isNotEmpty ?? false);

    showBottomSheetOrDialog(
      context,
      title: hasName ? Text(getDisplayName(profile, pubkey)) : null,
      builder: (sheetContext, scrollController) => MemberDetailSheet(
        pubkey: pubkey,
        profile: profile,
        isSelf: isMe,
        keyIndices: keyIndices,
        deviceKeys: deviceKeys,
        scrollController: scrollController,
        onRestoreFromBackup: canManageSelf
            ? () {
                Navigator.pop(sheetContext);
                WalletAddColumn.showAddKeyDialog(pageContext, asRef!);
              }
            : null,
      ),
    );
  }

  void _copyInviteLink(BuildContext context) {
    final secret = ChannelSecret.fromAccessStructureId(id: accessStructureId);
    copyToClipboard(secret.inviteLink());
  }

  void _confirmLeaveRemote(BuildContext context) {
    final nostr = NostrContext.of(context);
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final asRef = _asRef(context);

    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Leave remote wallet?'),
        content: const Text(
          'This disables Nostr coordination and returns the wallet '
          'to local-only mode. You can re-enable it later from '
          'the wallet settings.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              Navigator.pop(ctx);
              await nostr.nostrSettings.setCoordinationUiEnabled(
                accessStructureRef: asRef,
                enabled: false,
              );
              if (context.mounted) {
                Navigator.popUntil(context, (r) => r.isFirst);
              }
            },
            style: FilledButton.styleFrom(
              backgroundColor: Theme.of(context).colorScheme.error,
            ),
            child: const Text('Leave'),
          ),
        ],
      ),
    );
  }
}

class _MemberTile extends StatelessWidget {
  final PublicKey pubkey;
  final bool isYou;
  final List<int> shareIndices;
  final Color tileColor;
  final ShapeBorder shape;
  final EdgeInsetsGeometry contentPadding;
  final VoidCallback onTap;

  const _MemberTile({
    required this.pubkey,
    required this.isYou,
    required this.onTap,
    required this.tileColor,
    required this.shape,
    required this.contentPadding,
    this.shareIndices = const [],
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final builder = isYou
        ? (Widget Function(BuildContext, NostrProfile?) inner) =>
              MyProfileBuilder(
                builder: (ctx, _, profile) => inner(ctx, profile),
              )
        : (Widget Function(BuildContext, NostrProfile?) inner) =>
              ProfileBuilder(pubkey: pubkey, builder: inner);

    return builder((context, profile) {
      final subtitle = shareIndices.isNotEmpty
          ? Wrap(
              spacing: 4,
              children: shareIndices
                  .map(
                    (i) => Chip(
                      label: Text('#$i'),
                      labelStyle: theme.textTheme.labelSmall,
                      visualDensity: VisualDensity.compact,
                      materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      padding: EdgeInsets.zero,
                    ),
                  )
                  .toList(),
            )
          : Text(
              'holds no keys',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
                fontStyle: FontStyle.italic,
              ),
            );

      return ListTile(
        contentPadding: contentPadding,
        tileColor: tileColor,
        shape: shape,
        leading: NostrAvatar.medium(profile: profile, pubkey: pubkey),
        title: Row(
          children: [
            Flexible(child: Text(getDisplayName(profile, pubkey))),
            if (isYou) ...[
              const SizedBox(width: 8),
              Text(
                '(you)',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.primary,
                ),
              ),
            ],
          ],
        ),
        subtitle: subtitle,
        trailing: isYou
            ? const Icon(Icons.edit_rounded)
            : const Icon(Icons.chevron_right),
        onTap: onTap,
      );
    });
  }
}
