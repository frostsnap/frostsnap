import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';

class ProfileSettingsPage extends StatelessWidget {
  const ProfileSettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Nostr Profile')),
      body: MyProfileBuilder(
        builder: (context, pubkey, profile) {
          final npub = pubkey?.toNpub();

          return ListView(
            padding: const EdgeInsets.all(24),
            children: [
              Center(
                child: NostrAvatar.large(pubkey: pubkey, profile: profile),
              ),
              const SizedBox(height: 12),
              if (profile != null) ...[
                Center(
                  child: Text(
                    getDisplayName(profile, pubkey),
                    style: theme.textTheme.titleLarge,
                  ),
                ),
                const SizedBox(height: 4),
              ],
              const SizedBox(height: 12),
              if (npub != null) ...[
                _InfoSection(
                  label: 'Your npub',
                  value: npub,
                  onCopy: () => _copyToClipboard(context, npub, 'npub'),
                ),
              ],
              const SizedBox(height: 32),
              Text(
                'ADVANCED',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 16),
              OutlinedButton.icon(
                onPressed: () => _exportNsec(context),
                icon: const Icon(Icons.key),
                label: const Text('Export nsec (backup)'),
              ),
              const SizedBox(height: 12),
              OutlinedButton.icon(
                onPressed: () => _importNsec(context),
                icon: const Icon(Icons.swap_horiz),
                label: const Text('Import different nsec'),
              ),
              const SizedBox(height: 12),
              OutlinedButton.icon(
                onPressed: () => _generateNewNsec(context),
                icon: const Icon(Icons.casino),
                label: const Text('Generate new random identity'),
              ),
              const SizedBox(height: 12),
              if (pubkey != null)
                OutlinedButton.icon(
                  onPressed: () => _removeNsec(context),
                  icon: Icon(
                    Icons.person_remove_outlined,
                    color: theme.colorScheme.error,
                  ),
                  label: Text(
                    'Remove Nostr signing identity',
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                  style: OutlinedButton.styleFrom(
                    side: BorderSide(color: theme.colorScheme.error),
                  ),
                ),
              const SizedBox(height: 24),
              Container(
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: theme.colorScheme.errorContainer.withValues(
                    alpha: 0.3,
                  ),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Row(
                  children: [
                    Icon(
                      Icons.warning_amber_rounded,
                      color: theme.colorScheme.error,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Text(
                        'Your nsec is your Nostr private key. Never share it publicly.',
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.error,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
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

  void _exportNsec(BuildContext context) async {
    final nostr = NostrContext.of(context);

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Export nsec'),
        content: const Text(
          'Your nsec is your private key. Only share it with apps you trust. '
          'Anyone with your nsec can post as you.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Show nsec'),
          ),
        ],
      ),
    );

    if (confirmed != true || !context.mounted) return;

    try {
      final nsec = nostr.nostrSettings.getNsec();
      if (!context.mounted) return;

      await showDialog(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Your nsec'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: SelectableText(
                  nsec,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
                ),
              ),
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                child: OutlinedButton.icon(
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: nsec));
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('nsec copied to clipboard')),
                    );
                  },
                  icon: const Icon(Icons.copy),
                  label: const Text('Copy to clipboard'),
                ),
              ),
            ],
          ),
          actions: [
            FilledButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Done'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Error: $e')));
    }
  }

  void _importNsec(BuildContext context) async {
    final controller = TextEditingController();
    String? error;

    final result = await showDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: const Text('Import nsec'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'This will replace your current Nostr identity. '
                'Make sure you have backed up your current nsec if needed.',
              ),
              const SizedBox(height: 16),
              TextField(
                controller: controller,
                decoration: InputDecoration(
                  labelText: 'Enter nsec',
                  hintText: 'nsec1...',
                  border: const OutlineInputBorder(),
                  errorText: error,
                ),
                autocorrect: false,
                enableSuggestions: false,
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () {
                final nsec = controller.text.trim();
                if (nsec.isEmpty) {
                  setDialogState(() => error = 'Please enter an nsec');
                  return;
                }
                try {
                  Nsec.parse(s: nsec);
                  Navigator.of(dialogContext).pop(nsec);
                } catch (e) {
                  setDialogState(() => error = 'Invalid nsec format');
                }
              },
              child: const Text('Import'),
            ),
          ],
        ),
      ),
    );

    if (result == null || !context.mounted) return;

    try {
      await NostrContext.of(context).nostrSettings.setNsec(nsec: result);
      if (!context.mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('Nostr identity imported')));
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Error: $e')));
    }
  }

  void _generateNewNsec(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Generate new identity'),
        content: const Text(
          'This will replace your current Nostr identity with a new random one. '
          'Make sure you have backed up your current nsec if needed.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Generate'),
          ),
        ],
      ),
    );

    if (confirmed != true || !context.mounted) return;

    try {
      await NostrContext.of(context).nostrSettings.generate();
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('New Nostr identity generated')),
      );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Error: $e')));
    }
  }

  void _removeNsec(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove Nostr signing identity'),
        content: const Text(
          'Removes the nsec from this app. The app will no longer be able to '
          'send signed Nostr events until you set up an identity again.\n\n'
          'This does not delete chat history, and remote-coordinated wallets '
          'stay in remote mode (you can still read incoming messages).',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );

    if (confirmed != true || !context.mounted) return;

    try {
      await NostrContext.of(context).nostrSettings.clearNsec();
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Nostr signing identity removed')),
      );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Error: $e')));
    }
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
                    shortenNpub(value),
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontFamily: 'monospace',
                    ),
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
