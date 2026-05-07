import 'package:flutter/material.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';

enum NostrSetupResult { generated, imported, cancelled }

/// Shows the Nostr identity setup dialog.
/// Returns the setup result (generated, imported, or cancelled).
Future<NostrSetupResult> showNostrSetupDialog(BuildContext context) async {
  final result = await showDialog<NostrSetupResult>(
    context: context,
    barrierDismissible: true,
    builder: (context) => const _NostrSetupDialog(),
  );
  return result ?? NostrSetupResult.cancelled;
}

/// Ensures a Nostr identity exists, prompting the user if needed.
/// Returns true if identity is ready, false if the user cancelled.
Future<bool> ensureNostrIdentity(BuildContext context) async {
  final nostr = NostrContext.of(context);
  if (nostr.nostrSettings.hasIdentity()) return true;
  final result = await showNostrSetupDialog(context);
  return result != NostrSetupResult.cancelled;
}

class _NostrSetupDialog extends StatefulWidget {
  const _NostrSetupDialog();

  @override
  State<_NostrSetupDialog> createState() => _NostrSetupDialogState();
}

class _NostrSetupDialogState extends State<_NostrSetupDialog> {
  bool _showImport = false;
  final _nsecController = TextEditingController();
  String? _errorText;
  bool _isLoading = false;

  @override
  void dispose() {
    _nsecController.dispose();
    super.dispose();
  }

  void _generateIdentity() async {
    setState(() => _isLoading = true);
    try {
      await NostrContext.of(context).nostrSettings.generate();
      if (!mounted) return;
      Navigator.of(context).pop(NostrSetupResult.generated);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorText = 'Failed to generate identity: $e';
        _isLoading = false;
      });
    }
  }

  void _importIdentity() async {
    final nsec = _nsecController.text.trim();
    if (nsec.isEmpty) {
      setState(() => _errorText = 'Please enter your nsec');
      return;
    }

    setState(() {
      _isLoading = true;
      _errorText = null;
    });

    try {
      await NostrContext.of(context).nostrSettings.setNsec(nsec: nsec);
      if (!mounted) return;
      Navigator.of(context).pop(NostrSetupResult.imported);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorText = 'Failed to import nsec: $e';
        _isLoading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return _showImport ? _buildImport(context) : _buildWelcome(context);
  }

  Widget _buildImport(BuildContext context) {
    final theme = Theme.of(context);
    return AlertDialog(
      title: const Text('Import Nostr Identity'),
      content: SizedBox(
        width: 400,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Enter your nsec:'),
            const SizedBox(height: 12),
            TextField(
              controller: _nsecController,
              decoration: InputDecoration(
                hintText: 'nsec1...',
                errorText: _errorText,
                border: const OutlineInputBorder(),
              ),
              autocorrect: false,
              enableSuggestions: false,
              enabled: !_isLoading,
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Icon(
                  Icons.warning_amber_rounded,
                  color: theme.colorScheme.error,
                  size: 20,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    'Keep your nsec private! Never share it.',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _isLoading
              ? null
              : () => setState(() => _showImport = false),
          child: const Text('Back'),
        ),
        FilledButton(
          onPressed: _isLoading ? null : _importIdentity,
          child: _isLoading
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Import'),
        ),
      ],
    );
  }

  Widget _buildWelcome(BuildContext context) {
    final theme = Theme.of(context);
    return AlertDialog(
      title: const Text('Welcome to Frostsnap Chat'),
      content: SizedBox(
        width: 400,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.chat_bubble_outline,
              size: 64,
              color: theme.colorScheme.primary,
            ),
            const SizedBox(height: 16),
            Text(
              'To chat with your co-signers, you\'ll need a Nostr identity.',
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 24),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                onPressed: _isLoading ? null : _generateIdentity,
                icon: _isLoading
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.add),
                label: const Text('Generate New Identity'),
              ),
            ),
            const SizedBox(height: 12),
            SizedBox(
              width: double.infinity,
              child: OutlinedButton.icon(
                onPressed: _isLoading
                    ? null
                    : () => setState(() => _showImport = true),
                icon: const Icon(Icons.key),
                label: const Text('Import Existing nsec'),
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _isLoading
              ? null
              : () => Navigator.pop(context, NostrSetupResult.cancelled),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}
