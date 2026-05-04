import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:url_launcher/url_launcher.dart';

class MemberDetailSheet extends StatelessWidget {
  final PublicKey pubkey;
  final NostrProfile? profile;

  const MemberDetailSheet({
    super.key,
    required this.pubkey,
    required this.profile,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final npub = pubkey.toNpub();

    return DraggableScrollableSheet(
      initialChildSize: 0.5,
      minChildSize: 0.3,
      maxChildSize: 0.9,
      expand: false,
      builder: (context, scrollController) {
        return SingleChildScrollView(
          controller: scrollController,
          padding: const EdgeInsets.all(24),
          child: Column(
            children: [
              Container(
                width: 32,
                height: 4,
                margin: const EdgeInsets.only(bottom: 24),
                decoration: BoxDecoration(
                  color: theme.colorScheme.outline.withValues(alpha: 0.4),
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
              NostrAvatar.large(profile: profile, pubkey: pubkey),
              const SizedBox(height: 16),
              Text(
                getDisplayName(profile, pubkey),
                style: theme.textTheme.titleLarge,
              ),
              const SizedBox(height: 24),
              _InfoSection(
                label: 'npub',
                value: npub,
                onCopy: () => _copyToClipboard(context, npub, 'npub'),
              ),
              if (profile?.nip05 != null && profile!.nip05!.isNotEmpty) ...[
                const Divider(height: 32),
                _InfoSection(
                  label: 'Nostr Address (NIP-05)',
                  value: profile!.nip05!,
                  onCopy: () =>
                      _copyToClipboard(context, profile!.nip05!, 'address'),
                ),
              ],
              if (profile?.about != null && profile!.about!.isNotEmpty) ...[
                const Divider(height: 32),
                Align(
                  alignment: Alignment.centerLeft,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'About',
                        style: theme.textTheme.labelMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Text(profile!.about!, style: theme.textTheme.bodyMedium),
                    ],
                  ),
                ),
              ],
              const SizedBox(height: 32),
              SizedBox(
                width: double.infinity,
                child: OutlinedButton.icon(
                  onPressed: () => _openInNostrClient(context, npub),
                  icon: const Icon(Icons.open_in_new),
                  label: const Text('Open in Nostr Client'),
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  void _copyToClipboard(BuildContext context, String value, String label) {
    Clipboard.setData(ClipboardData(text: value));
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text('$label copied to clipboard'),
        duration: const Duration(seconds: 2),
      ),
    );
  }

  void _openInNostrClient(BuildContext context, String npub) {
    final uri = Uri.parse('nostr:$npub');
    launchUrl(uri).catchError((e) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('No Nostr client installed'),
          duration: Duration(seconds: 2),
        ),
      );
      return false;
    });
  }
}

class _InfoSection extends StatelessWidget {
  final String label;
  final String value;
  final VoidCallback onCopy;

  const _InfoSection({
    required this.label,
    required this.value,
    required this.onCopy,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: theme.textTheme.labelMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 8),
        InkWell(
          onTap: onCopy,
          borderRadius: BorderRadius.circular(8),
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
            decoration: BoxDecoration(
              color: theme.colorScheme.surfaceContainerHighest,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    value,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontFamily: 'monospace',
                    ),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 8),
                Icon(
                  Icons.copy,
                  size: 18,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
