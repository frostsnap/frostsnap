import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/member_detail_sheet.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/profile_settings_page.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';

class GroupInfoPage extends StatelessWidget {
  final String walletName;
  final List<PublicKey> members;
  final AccessStructureId accessStructureId;

  const GroupInfoPage({
    super.key,
    required this.walletName,
    required this.members,
    required this.accessStructureId,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final nostrState = NostrContext.of(context);
    final myPubkey = nostrState.myPubkey;

    final otherMembers = members
        .where((m) => myPubkey == null || m != myPubkey)
        .toList();

    return Scaffold(
      appBar: AppBar(title: const Text('Group Info')),
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
          const SizedBox(height: 24),
          if (otherMembers.isNotEmpty) ...[
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
                onTap: () => _showMemberDetail(context, member),
              ),
            ),
          ],
          const Divider(height: 32),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Text(
              'YOUR PROFILE',
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          MyProfileBuilder(
            builder: (context, pubkey, profile) {
              return ListTile(
                leading: NostrAvatar.medium(profile: profile, pubkey: pubkey),
                title: Text(
                  pubkey != null
                      ? getDisplayName(profile, pubkey)
                      : 'Not configured',
                ),
                subtitle: const Text('Edit your Nostr profile'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _openProfileSettings(context),
              );
            },
          ),
          const Divider(height: 32),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Text(
              'INVITE LINK',
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          ListTile(
            leading: const Icon(Icons.link_rounded),
            title: const Text('Copy invite link'),
            subtitle: const Text('Share this link to invite others to join'),
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
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (context) => MemberDetailSheet(pubkey: pubkey, profile: profile),
    );
  }

  void _copyInviteLink(BuildContext context) {
    final secret = ChannelSecret.fromAccessStructureId(id: accessStructureId);
    final link = secret.inviteLink();
    Clipboard.setData(ClipboardData(text: link));
    showMessageSnackbar(context, 'Invite link copied to clipboard');
  }

  void _openProfileSettings(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute(builder: (context) => const ProfileSettingsPage()),
    );
  }
}

class _MemberTile extends StatelessWidget {
  final PublicKey pubkey;
  final VoidCallback onTap;

  const _MemberTile({required this.pubkey, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return ProfileBuilder(
      pubkey: pubkey,
      builder: (context, profile) {
        return ListTile(
          leading: NostrAvatar.medium(profile: profile, pubkey: pubkey),
          title: Text(getDisplayName(profile, pubkey)),
          subtitle: Text(
            shortenNpub(pubkey.toNpub()),
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
          trailing: const Icon(Icons.chevron_right),
          onTap: onTap,
        );
      },
    );
  }
}
