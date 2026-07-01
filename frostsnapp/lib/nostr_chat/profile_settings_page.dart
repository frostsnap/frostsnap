import 'package:flutter/material.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';

class ProfileSettingsPage extends StatefulWidget {
  const ProfileSettingsPage({super.key});

  @override
  State<ProfileSettingsPage> createState() => _ProfileSettingsPageState();
}

class _ProfileSettingsPageState extends State<ProfileSettingsPage> {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Nostr Profile')),
      body: MyProfileBuilder(
        builder: (context, pubkey, _) {
          final nostr = NostrContext.of(context);
          final identity = nostr.nostrSettings.currentIdentity();
          return ListView(
            padding: const EdgeInsets.all(24),
            children: [
              if (identity is NostrIdentity_Imported)
                _ImportedModeSection(
                  key: ValueKey(('imported', identityPubkey(identity: identity).toHex())),
                  identity: identity,
                  onRefreshed: () => setState(() {}),
                )
              else if (identity is NostrIdentity_Generated)
                _GeneratedModeSection(
                  key: ValueKey(('generated', identityPubkey(identity: identity).toHex())),
                  identity: identity,
                )
              else
                _NoIdentitySection(pubkey: pubkey),
              const SizedBox(height: 32),
              _AdvancedSection(onChanged: () => setState(() {})),
            ],
          );
        },
      ),
    );
  }
}

// =============================================================================
// Mode A — read-only public profile
// =============================================================================

class _ImportedModeSection extends StatefulWidget {
  final NostrIdentity_Imported identity;
  final VoidCallback onRefreshed;
  const _ImportedModeSection({
    super.key,
    required this.identity,
    required this.onRefreshed,
  });

  @override
  State<_ImportedModeSection> createState() => _ImportedModeSectionState();
}

class _ImportedModeSectionState extends State<_ImportedModeSection> {
  bool _refreshing = false;

  Future<void> _refresh() async {
    if (_refreshing) return;
    setState(() => _refreshing = true);
    try {
      final nostr = NostrContext.of(context);
      final client = await nostr.nostrClient;
      // Re-fetch the public kind 0 from relays (5s runtime timeout)
      // and re-persist it as the new cached_public_profile.
      final fresh = await client.fetchProfile(pubkey: identityPubkey(identity: widget.identity));
      if (!mounted) return;
      if (fresh == null) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('No public profile found.')),
        );
        return;
      }
      final nsec = nostr.nostrSettings.getNsec();
      await nostr.nostrSettings.setImportedIdentity(
        nsec: nsec,
        cachedPublicProfile: fresh,
      );
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Profile refreshed')),
      );
      widget.onRefreshed();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Refresh failed: $e')),
      );
    } finally {
      if (mounted) setState(() => _refreshing = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final profile = widget.identity.cachedProfile;
    final npub = identityPubkey(identity: widget.identity).toNpub();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Center(
          child: NostrAvatar.large(
            pubkey: identityPubkey(identity: widget.identity),
            profile: profile,
          ),
        ),
        const SizedBox(height: 12),
        Center(
          child: Text(
            getDisplayName(profile, identityPubkey(identity: widget.identity)),
            style: theme.textTheme.titleLarge,
          ),
        ),
        const SizedBox(height: 8),
        Center(
          child: Text(
            'Your profile is managed via your other nostr clients.\n'
            'Edits made there propagate to your groups.',
            textAlign: TextAlign.center,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        const SizedBox(height: 12),
        Center(
          child: OutlinedButton.icon(
            onPressed: _refreshing ? null : _refresh,
            icon: _refreshing
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.refresh),
            label: const Text('Refresh from relays'),
          ),
        ),
        const SizedBox(height: 16),
        _InfoSection(
          label: 'Your npub',
          value: npub,
          onCopy: () => copyToClipboard(npub),
        ),
      ],
    );
  }
}

// =============================================================================
// Mode B — read-only name display
// =============================================================================

class _GeneratedModeSection extends StatelessWidget {
  final NostrIdentity_Generated identity;
  const _GeneratedModeSection({super.key, required this.identity});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final npub = identityPubkey(identity: identity).toNpub();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Center(
          child: NostrAvatar.large(pubkey: identityPubkey(identity: identity), profile: null),
        ),
        const SizedBox(height: 12),
        Center(
          child: Text(identity.name, style: theme.textTheme.titleLarge),
        ),
        const SizedBox(height: 8),
        Center(
          child: Text(
            'To change your name, generate a new identity below.',
            textAlign: TextAlign.center,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        const SizedBox(height: 16),
        _InfoSection(
          label: 'Your npub',
          value: npub,
          onCopy: () => copyToClipboard(npub),
        ),
      ],
    );
  }
}

// =============================================================================
// No identity yet (shouldn't normally reach this page in that state, but
// render something sensible if we do)
// =============================================================================

class _NoIdentitySection extends StatelessWidget {
  final dynamic pubkey;
  const _NoIdentitySection({required this.pubkey});

  @override
  Widget build(BuildContext context) {
    return const Center(child: Text('No identity configured.'));
  }
}

// =============================================================================
// Advanced — identity-management buttons (mode-agnostic)
// =============================================================================

class _AdvancedSection extends StatelessWidget {
  final VoidCallback onChanged;
  const _AdvancedSection({required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
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
            color: theme.colorScheme.errorContainer.withValues(alpha: 0.3),
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
  }

  Future<void> _exportNsec(BuildContext context) async {
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
                  onPressed: () => copyToClipboard(nsec),
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

  Future<void> _importNsec(BuildContext context) async {
    final controller = TextEditingController();
    String? error;
    bool busy = false;

    final completed = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: const Text('Import nsec'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'This will replace your current Nostr identity. '
                'The imported nsec must already have a public profile '
                '(NIP-01 kind 0) published from your usual nostr client.',
              ),
              const SizedBox(height: 16),
              TextField(
                controller: controller,
                enabled: !busy,
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
              onPressed: busy ? null : () => Navigator.of(dialogContext).pop(false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: busy
                  ? null
                  : () async {
                      final nsec = controller.text.trim();
                      if (nsec.isEmpty) {
                        setDialogState(() => error = 'Please enter an nsec');
                        return;
                      }
                      try {
                        Nsec.parse(s: nsec);
                      } catch (_) {
                        setDialogState(() => error = 'Invalid nsec format');
                        return;
                      }
                      setDialogState(() {
                        busy = true;
                        error = null;
                      });
                      try {
                        final nostr = NostrContext.of(context);
                        final client = await nostr.nostrClient;
                        final cached = await client.fetchProfileForImport(nsec: nsec);
                        await nostr.nostrSettings.setImportedIdentity(
                          nsec: nsec,
                          cachedPublicProfile: cached,
                        );
                        // Mode A doesn't publish in-channel — wipe
                        // any prior Mode-B publish snapshot.
                        if (!dialogContext.mounted) return;
                        Navigator.of(dialogContext).pop(true);
                      } catch (e) {
                        setDialogState(() {
                          busy = false;
                          error = '$e';
                        });
                      }
                    },
              child: busy
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('Import'),
            ),
          ],
        ),
      ),
    );

    if (completed == true && context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Nostr identity imported')),
      );
      onChanged();
    }
  }

  Future<void> _generateNewNsec(BuildContext context) async {
    final controller = TextEditingController();
    String? error;
    bool busy = false;

    final completed = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: const Text('Generate new identity'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'This will replace your current Nostr identity with a new '
                'random one. Make sure you have backed up your current nsec '
                'if needed.\n\n'
                'Pick a display name — other members of your groups will see '
                'this on your messages.',
              ),
              const SizedBox(height: 16),
              TextField(
                controller: controller,
                enabled: !busy,
                autofocus: true,
                decoration: InputDecoration(
                  labelText: 'Display name',
                  errorText: error,
                  border: const OutlineInputBorder(),
                ),
                textInputAction: TextInputAction.done,
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: busy ? null : () => Navigator.of(dialogContext).pop(false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: busy
                  ? null
                  : () async {
                      final name = controller.text.trim();
                      if (name.isEmpty) {
                        setDialogState(() => error = 'Please enter a display name');
                        return;
                      }
                      setDialogState(() {
                        busy = true;
                        error = null;
                      });
                      try {
                        final nostr = NostrContext.of(context);
                        await nostr.nostrSettings.generateNewIdentity(name: name);
                        if (!dialogContext.mounted) return;
                        Navigator.of(dialogContext).pop(true);
                      } catch (e) {
                        setDialogState(() {
                          busy = false;
                          error = '$e';
                        });
                      }
                    },
              child: busy
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('Generate'),
            ),
          ],
        ),
      ),
    );

    if (completed == true && context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('New Nostr identity generated')),
      );
      onChanged();
    }
  }

  Future<void> _removeNsec(BuildContext context) async {
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
      final nostr = NostrContext.of(context);
      await nostr.nostrSettings.clearIdentity();
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Nostr signing identity removed')),
      );
      onChanged();
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
