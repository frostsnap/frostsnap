import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/theme.dart';

const _rulesPath = '/etc/udev/rules.d/99-frostsnap.rules';
const _rulesContent = '''# Frostsnap USB device
SUBSYSTEM=="tty", SUBSYSTEMS=="usb", ATTRS{idVendor}=="303a", ATTRS{idProduct}=="1001", MODE="0666"
''';

const _manualCommandsCopy =
    "echo 'SUBSYSTEM==\"tty\", SUBSYSTEMS==\"usb\", ATTRS{idVendor}==\"303a\", ATTRS{idProduct}==\"1001\", MODE=\"0666\"' | sudo tee $_rulesPath\n"
    'sudo udevadm control --reload-rules';

const _manualCommandsDisplay =
    "\$ echo 'SUBSYSTEM==\"tty\", SUBSYSTEMS==\"usb\", ATTRS{idVendor}==\"303a\", ATTRS{idProduct}==\"1001\", MODE=\"0666\"' | sudo tee $_rulesPath\n"
    '\$ sudo udevadm control --reload-rules';

Future<bool> _areRulesInstalled() async {
  return File(_rulesPath).exists();
}

Future<bool> _installUdevRules() async {
  final result = await Process.run('pkexec', [
    'sh',
    '-c',
    "echo '$_rulesContent' > $_rulesPath && udevadm control --reload-rules",
  ]);
  return result.exitCode == 0;
}

Future<bool> _removeUdevRules() async {
  final result = await Process.run('pkexec', [
    'sh',
    '-c',
    'rm -f $_rulesPath && udevadm control --reload-rules',
  ]);
  return result.exitCode == 0;
}

class UdevSetupPage extends StatefulWidget {
  const UdevSetupPage({super.key});

  @override
  State<UdevSetupPage> createState() => _UdevSetupPageState();
}

class _UdevSetupPageState extends State<UdevSetupPage>
    with SingleTickerProviderStateMixin {
  bool? _rulesInstalled;
  bool _installing = false;
  bool _removing = false;
  late final AnimationController _refreshController;

  @override
  void initState() {
    super.initState();
    _refreshController = AnimationController(
      duration: const Duration(milliseconds: 500),
      vsync: this,
    );
    _checkRulesStatus();
  }

  @override
  void dispose() {
    _refreshController.dispose();
    super.dispose();
  }

  Future<void> _checkRulesStatus() async {
    final installed = await _areRulesInstalled();
    if (mounted) {
      setState(() {
        _rulesInstalled = installed;
      });
    }
  }

  Future<void> _handleInstall() async {
    setState(() {
      _installing = true;
    });

    final success = await _installUdevRules();

    if (mounted) {
      setState(() {
        _installing = false;
      });

      await _checkRulesStatus();

      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            success
                ? 'udev rules installed. Please re-plug your device.'
                : 'Installation cancelled or failed.',
          ),
        ),
      );
    }
  }

  Future<void> _handleRemove() async {
    setState(() {
      _removing = true;
    });

    final success = await _removeUdevRules();

    if (mounted) {
      setState(() {
        _removing = false;
      });

      await _checkRulesStatus();

      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            success ? 'udev rules removed.' : 'Removal cancelled or failed.',
          ),
        ),
      );
    }
  }

  bool get _busy => _installing || _removing;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isInstalled = _rulesInstalled == true;
    final isLoading = _rulesInstalled == null;

    return SettingsContent(
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: ListView(
          children: [
            Text(
              'Linux requires udev rules to allow the app to communicate with Frostsnap devices without root permissions.',
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 24),
            _buildStatusCard(theme, isInstalled, isLoading),
            const SizedBox(height: 12),
            _buildAutoInstallCard(theme, isInstalled, isLoading),
            const SizedBox(height: 12),
            _buildManualCard(theme),
          ],
        ),
      ),
    );
  }

  Widget _buildStatusCard(ThemeData theme, bool isInstalled, bool isLoading) {
    return Card.filled(
      margin: EdgeInsets.zero,
      color: isLoading
          ? theme.colorScheme.surfaceContainerHighest
          : isInstalled
          ? theme.colorScheme.primaryContainer
          : theme.colorScheme.errorContainer,
      child: ListTile(
        leading: isLoading
            ? SizedBox(
                width: 24,
                height: 24,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : Icon(
                isInstalled ? Icons.check_circle : Icons.warning,
                color: isInstalled
                    ? theme.colorScheme.onPrimaryContainer
                    : theme.colorScheme.onErrorContainer,
              ),
        title: Text(
          isLoading
              ? 'Checking status...'
              : isInstalled
              ? 'udev rules are installed'
              : 'udev rules not installed',
          style: TextStyle(
            color: isLoading
                ? null
                : isInstalled
                ? theme.colorScheme.onPrimaryContainer
                : theme.colorScheme.onErrorContainer,
          ),
        ),
        trailing: RotationTransition(
          turns: _refreshController,
          child: IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () {
              _refreshController.forward(from: 0);
              _checkRulesStatus();
            },
            tooltip: 'Refresh',
          ),
        ),
      ),
    );
  }

  Widget _buildAutoInstallCard(
    ThemeData theme,
    bool isInstalled,
    bool isLoading,
  ) {
    return Card(
      margin: EdgeInsets.zero,
      child: ListTile(
        leading: Icon(Icons.auto_fix_high, color: theme.colorScheme.primary),
        title: Text('Automatic'),
        subtitle: Text('Install with one click'),
        trailing: isLoading
            ? null
            : !isInstalled
            ? FilledButton.icon(
                onPressed: _busy ? null : _handleInstall,
                icon: _installing
                    ? SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: theme.colorScheme.onPrimary,
                        ),
                      )
                    : const Icon(Icons.install_desktop),
                label: Text(_installing ? 'Installing...' : 'Install'),
              )
            : OutlinedButton.icon(
                onPressed: _busy ? null : _handleRemove,
                icon: _removing
                    ? SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.delete_outline),
                label: Text(_removing ? 'Removing...' : 'Remove'),
              ),
      ),
    );
  }

  Widget _buildManualCard(ThemeData theme) {
    return Card(
      margin: EdgeInsets.zero,
      child: ExpansionTile(
        leading: Icon(Icons.terminal, color: theme.colorScheme.primary),
        title: Text('Manual'),
        subtitle: Text('Run these commands in the terminal'),
        shape: Border(),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _buildCodeBlock(
                  theme,
                  _manualCommandsDisplay,
                  onCopy: () {
                    Clipboard.setData(ClipboardData(text: _manualCommandsCopy));
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('Copied to clipboard')),
                    );
                  },
                ),
                const SizedBox(height: 12),
                Text(
                  'Then re-plug your Frostsnap device.',
                  style: theme.textTheme.bodyMedium,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildCodeBlock(ThemeData theme, String code, {VoidCallback? onCopy}) {
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Stack(
        children: [
          Padding(
            padding: const EdgeInsets.all(16),
            child: SelectableText(code, style: monospaceTextStyle),
          ),
          if (onCopy != null)
            Positioned(
              top: 4,
              right: 4,
              child: IconButton(
                icon: const Icon(Icons.copy, size: 18),
                onPressed: onCopy,
                tooltip: 'Copy',
              ),
            ),
        ],
      ),
    );
  }
}
