import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/nostr_chat/member_detail_sheet.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/wallet_more.dart';

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

  List<int> _shareIndicesFor(PublicKey pubkey) {
    final hex = pubkey.toHex();
    for (final p in participantShares) {
      if (p.pubkey.toHex() == hex) return p.shareIndices.toList();
    }
    return const [];
  }

  int _totalShares() {
    return participantShares.fold<int>(
      0,
      (sum, p) => sum + p.shareIndices.length,
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final nostrState = NostrContext.of(context);
    final myPubkey = nostrState.myPubkey;
    final walletCtx = WalletContext.of(context);

    final otherMembers = members
        .where((m) => m != myPubkey)
        .toList();

    final frostKey = walletCtx != null
        ? coord.getFrostKey(keyId: walletCtx.keyId)
        : null;
    final threshold = frostKey
            ?.getAccessStructure(accessStructureId: accessStructureId)
            ?.threshold() ??
        0;
    final totalShares = _totalShares();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Group Info'),
        actions: [
          if (walletCtx != null)
            IconButton(
              icon: const Icon(Icons.code_rounded),
              tooltip: 'Show descriptor',
              onPressed: () => showExportWalletDialog(
                context,
                walletCtx.network.descriptorForKey(
                  masterAppkey: walletCtx.wallet.masterAppkey,
                ),
              ),
            ),
        ],
      ),
      body: ListView(
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
            child: Text(
              '$walletName Chat',
              style: theme.textTheme.headlineSmall,
            ),
          ),
          Center(
            child: Text(
              '${members.length} member${members.length != 1 ? 's' : ''}',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          if (threshold > 0 && totalShares > 0)
            Center(
              child: Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Text(
                  '$threshold-of-$totalShares threshold',
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
          const SizedBox(height: 24),
          // Your profile first
          MyProfileBuilder(
            builder: (context, pubkey, profile) {
              final myShares = _shareIndicesFor(pubkey);
              return ListTile(
                leading: NostrAvatar.medium(profile: profile, pubkey: pubkey),
                title: Row(
                  children: [
                    Flexible(
                      child: Text(getDisplayName(profile, pubkey)),
                    ),
                    const SizedBox(width: 8),
                    Text('(you)', style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.primary,
                    )),
                  ],
                ),
                subtitle: myShares.isNotEmpty
                    ? Wrap(
                        spacing: 4,
                        children: myShares
                            .map((i) => Chip(
                                  label: Text('#$i'),
                                  visualDensity: VisualDensity.compact,
                                  materialTapTargetSize:
                                      MaterialTapTargetSize.shrinkWrap,
                                ))
                            .toList(),
                      )
                    : const Text('Edit your Nostr profile'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _showMemberDetail(context, pubkey),
              );
            },
          ),
          ListTile(
            leading: Icon(Icons.exit_to_app, color: theme.colorScheme.error),
            title: Text(
              'Leave remote wallet',
              style: TextStyle(color: theme.colorScheme.error),
            ),
            subtitle: const Text('Switch back to local coordination'),
            onTap: () => _confirmLeaveRemote(context),
          ),
          if (otherMembers.isNotEmpty) ...[
            const Divider(height: 32),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Text(
                'MEMBERS',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            ...otherMembers.map(
              (member) => _MemberTile(
                pubkey: member,
                shareIndices: _shareIndicesFor(member),
                onTap: () => _showMemberDetail(context, member),
              ),
            ),
          ],
          ListTile(
            leading: const Icon(Icons.person_add_rounded),
            title: const Text('Invite someone'),
            subtitle: const Text('Copy invite link to share'),
            trailing: const Icon(Icons.copy_rounded),
            onTap: () => _copyInviteLink(context),
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }

  void _showMemberDetail(BuildContext context, PublicKey pubkey) {
    final profile = NostrContext.of(context).getProfile(pubkey);
    final nostr = NostrContext.of(context);
    final isMe = pubkey == nostr.myPubkey;
    final shares = _shareIndicesFor(pubkey);

    // For own profile, look up device names per share from the local
    // access structure.
    final deviceNames = <int, String>{};
    if (isMe) {
      final walletCtx = WalletContext.of(context);
      if (walletCtx != null) {
        final accessStruct = coord.getAccessStructure(
          asRef: AccessStructureRef(
            keyId: walletCtx.keyId,
            accessStructureId: accessStructureId,
          ),
        );
        if (accessStruct != null) {
          for (final deviceId in accessStruct.devices()) {
            final idx = accessStruct.getDeviceShortShareIndex(deviceId: deviceId);
            if (idx != null && shares.contains(idx)) {
              deviceNames[idx] = coord.getDeviceName(id: deviceId) ?? '<unknown>';
            }
          }
        }
      }
    }

    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (context) => MemberDetailSheet(
        pubkey: pubkey,
        profile: profile,
        shareIndices: shares,
        deviceNamesPerShare: deviceNames,
      ),
    );
  }

  void _copyInviteLink(BuildContext context) {
    final secret = ChannelSecret.fromAccessStructureId(id: accessStructureId);
    final link = secret.inviteLink();
    Clipboard.setData(ClipboardData(text: link));
    showMessageSnackbar(context, 'Invite link copied to clipboard');
  }

  void _confirmLeaveRemote(BuildContext context) {
    final nostr = NostrContext.of(context);
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final asRef = AccessStructureRef(
      keyId: walletCtx.keyId,
      accessStructureId: accessStructureId,
    );

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
  final List<int> shareIndices;
  final VoidCallback onTap;

  const _MemberTile({
    required this.pubkey,
    required this.onTap,
    this.shareIndices = const [],
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ProfileBuilder(
      pubkey: pubkey,
      builder: (context, profile) {
        return ListTile(
          leading: NostrAvatar.medium(profile: profile, pubkey: pubkey),
          title: Text(getDisplayName(profile, pubkey)),
          subtitle: shareIndices.isNotEmpty
              ? Wrap(
                  spacing: 4,
                  children: shareIndices
                      .map(
                        (i) => Chip(
                          label: Text('#$i'),
                          labelStyle: theme.textTheme.labelSmall,
                          visualDensity: VisualDensity.compact,
                          materialTapTargetSize:
                              MaterialTapTargetSize.shrinkWrap,
                          padding: EdgeInsets.zero,
                        ),
                      )
                      .toList(),
                )
              : Text(
                  shortenNpub(pubkey.toNpub()),
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
          trailing: const Icon(Icons.chevron_right),
          onTap: onTap,
        );
      },
    );
  }
}

