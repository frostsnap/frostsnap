import 'package:flutter/material.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/progress_indicator.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/tofu_certificate_dialog.dart';
import 'package:rxdart/rxdart.dart';

class ElectrumServerSettingsPage extends StatelessWidget {
  const ElectrumServerSettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final settings = SettingsContext.of(context)!;
    final settingsStream = Rx.combineLatest2(
      settings.electrumSettings,
      settings.developerSettings,
      (ElectrumSettings electrum, DeveloperSettings developer) {
        return (
          developerMode: developer.developerMode,
          servers: electrum.electrumServers,
        );
      },
    );

    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: StreamBuilder(
        stream: settingsStream,
        builder: (context, snap) {
          final servers = snap.data?.servers ?? [];
          final developerMode = snap.data?.developerMode ?? false;

          return ListView(
            children: servers.map((record) {
              final network = record.network;
              if (!network.isMainnet() && !developerMode) {
                return const SizedBox.shrink();
              }

              return _NetworkServerCard(
                network: network,
                primaryUrl: record.url,
                backupUrl: record.backupUrl,
                enabled: record.enabled,
              );
            }).toList(),
          );
        },
      ),
    );
  }
}

class _NetworkServerCard extends StatelessWidget {
  final BitcoinNetwork network;
  final String primaryUrl;
  final String backupUrl;
  final ElectrumEnabled enabled;

  const _NetworkServerCard({
    required this.network,
    required this.primaryUrl,
    required this.backupUrl,
    required this.enabled,
  });

  ChainStatusState? _getServerStatus(
    ChainStatusState? connectionState,
    String? connectedUrl,
    String serverUrl,
  ) {
    if (connectionState == null) return null;
    if (connectionState == ChainStatusState.idle) return ChainStatusState.idle;
    if (connectionState == ChainStatusState.connected &&
        connectedUrl == serverUrl) {
      return ChainStatusState.connected;
    }
    if (connectionState == ChainStatusState.connecting) {
      return ChainStatusState.connecting;
    }
    if (connectionState == ChainStatusState.disconnected) {
      return ChainStatusState.disconnected;
    }
    return ChainStatusState.idle;
  }

  @override
  Widget build(BuildContext context) {
    final settings = SettingsContext.of(context)!;
    final primaryEnabled = enabled != ElectrumEnabled.none;
    final backupEnabled = enabled == ElectrumEnabled.all;

    return StreamBuilder<ChainStatus>(
      stream: settings.chainStatusStream(network),
      builder: (context, chainSnap) {
        final chainStatus = chainSnap.data;
        final connectedUrl = chainStatus?.electrumUrl;
        final connectionState = chainStatus?.state;

        return Card(
          margin: const EdgeInsets.only(bottom: 16),
          clipBehavior: Clip.antiAlias,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
                child: Text(
                  network.name(),
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              ServerListTile(
                network: network,
                url: primaryUrl,
                isBackup: false,
                status: _getServerStatus(
                  connectionState,
                  connectedUrl,
                  primaryUrl,
                ),
                enabled: primaryEnabled,
                onEnabledChanged: (value) async {
                  final newEnabled = value
                      ? ElectrumEnabled.primaryOnly
                      : ElectrumEnabled.none;
                  await settings.settings.setElectrumEnabled(
                    network: network,
                    enabled: newEnabled,
                  );
                },
              ),
              Center(
                child: IconButton(
                  icon: const Icon(Icons.swap_vert),
                  tooltip: 'Swap primary and backup',
                  onPressed: () async {
                    await settings.settings.setElectrumServers(
                      network: network,
                      primary: backupUrl,
                      backup: primaryUrl,
                    );
                  },
                ),
              ),
              ServerListTile(
                network: network,
                url: backupUrl,
                isBackup: true,
                status: _getServerStatus(
                  connectionState,
                  connectedUrl,
                  backupUrl,
                ),
                enabled: backupEnabled,
                onEnabledChanged: primaryEnabled
                    ? (value) async {
                        final newEnabled = value
                            ? ElectrumEnabled.all
                            : ElectrumEnabled.primaryOnly;
                        await settings.settings.setElectrumEnabled(
                          network: network,
                          enabled: newEnabled,
                        );
                      }
                    : null,
              ),
            ],
          ),
        );
      },
    );
  }
}

class ServerListTile extends StatelessWidget {
  final BitcoinNetwork network;
  final String url;
  final bool isBackup;
  final ChainStatusState? status;
  final bool enabled;
  final ValueChanged<bool>? onEnabledChanged;

  const ServerListTile({
    super.key,
    required this.network,
    required this.url,
    required this.isBackup,
    required this.status,
    required this.enabled,
    this.onEnabledChanged,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final Color statusColor;
    if (!enabled) {
      statusColor = theme.colorScheme.outline;
    } else {
      switch (status) {
        case ChainStatusState.connected:
          statusColor = theme.colorScheme.primary;
          break;
        case ChainStatusState.connecting:
          statusColor = theme.colorScheme.tertiary;
          break;
        case ChainStatusState.disconnected:
          statusColor = theme.colorScheme.error;
          break;
        case ChainStatusState.idle:
        case null:
          statusColor = theme.colorScheme.outline;
          break;
      }
    }

    return ListTile(
      leading: Container(
        width: 12,
        height: 12,
        decoration: BoxDecoration(shape: BoxShape.circle, color: statusColor),
      ),
      title: Text(isBackup ? 'Backup Server' : 'Primary Server'),
      subtitle: Text(url, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Switch(value: enabled, onChanged: onEnabledChanged),
          const Icon(Icons.chevron_right),
        ],
      ),
      onTap: () => _showEditDialog(context),
    );
  }

  void _showEditDialog(BuildContext context) {
    showBottomSheetOrDialog(
      context,
      title: Text(isBackup ? 'Edit Backup Server' : 'Edit Primary Server'),
      builder: (context, scrollController) {
        return EditServerDialog(
          network: network,
          initialUrl: url,
          isBackup: isBackup,
          scrollController: scrollController,
        );
      },
    );
  }
}

enum _EditServerState { input, connecting, certificate, success, failure }

class EditServerDialog extends StatefulWidget {
  final BitcoinNetwork network;
  final String initialUrl;
  final bool isBackup;
  final ScrollController scrollController;

  const EditServerDialog({
    super.key,
    required this.network,
    required this.initialUrl,
    required this.isBackup,
    required this.scrollController,
  });

  @override
  State<EditServerDialog> createState() => _EditServerDialogState();
}

class _EditServerDialogState extends State<EditServerDialog> {
  late TextEditingController _controller;
  _EditServerState _state = _EditServerState.input;
  String? _errorMessage;
  UntrustedCertificate? _certificateInfo;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initialUrl);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _connect() async {
    setState(() {
      _state = _EditServerState.connecting;
      _errorMessage = null;
    });

    try {
      final settingsCtx = SettingsContext.of(context)!;
      final result = await settingsCtx.settings.checkAndSetElectrumServer(
        network: widget.network,
        url: _controller.text,
        isBackup: widget.isBackup,
      );
      await _handleResult(result);
    } catch (e) {
      setState(() {
        _state = _EditServerState.failure;
        _errorMessage = e.toString();
      });
    }
  }

  Future<void> _handleResult(ConnectionResult result) async {
    switch (result) {
      case ConnectionResult_Success():
        setState(() {
          _state = _EditServerState.success;
        });
        break;

      case ConnectionResult_CertificatePromptNeeded(:final field0):
        setState(() {
          _state = _EditServerState.certificate;
          _certificateInfo = field0;
        });
        break;

      case ConnectionResult_Failed(:final field0):
        setState(() {
          _state = _EditServerState.failure;
          _errorMessage = field0;
        });
        break;
    }
  }

  Future<void> _acceptCertificate() async {
    if (_certificateInfo == null) return;

    setState(() {
      _state = _EditServerState.connecting;
    });

    try {
      final settingsCtx = SettingsContext.of(context)!;
      final result = await settingsCtx.settings.acceptCertificateAndRetry(
        network: widget.network,
        serverUrl: _controller.text,
        certificate: _certificateInfo!.certificateDer,
        isBackup: widget.isBackup,
      );
      await _handleResult(result);
    } catch (e) {
      setState(() {
        _state = _EditServerState.failure;
        _errorMessage = e.toString();
      });
    }
  }

  void _rejectCertificate() {
    setState(() {
      _state = _EditServerState.input;
      _certificateInfo = null;
    });
  }

  void _restoreDefault() {
    final defaultUrl = widget.isBackup
        ? widget.network.defaultBackupElectrumServer()
        : widget.network.defaultElectrumServer();
    setState(() => _controller.text = defaultUrl);
  }

  void _tryAgain() {
    setState(() {
      _state = _EditServerState.input;
      _errorMessage = null;
    });
  }

  void _cancel() {
    setState(() {
      _state = _EditServerState.input;
    });
  }

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      controller: widget.scrollController,
      child: AnimatedSwitcher(
        duration: Durations.medium4,
        transitionBuilder: (child, animation) {
          return FadeTransition(opacity: animation, child: child);
        },
        child: _buildContent(),
      ),
    );
  }

  Widget _buildContent() {
    switch (_state) {
      case _EditServerState.input:
        return _buildInputContent();
      case _EditServerState.connecting:
        return _buildConnectingContent();
      case _EditServerState.certificate:
        return _buildCertificateContent();
      case _EditServerState.success:
        return _buildSuccessContent();
      case _EditServerState.failure:
        return _buildFailureContent();
    }
  }

  Widget _buildInputContent() {
    return Padding(
      key: const ValueKey(_EditServerState.input),
      padding: const EdgeInsets.all(16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            controller: _controller,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Server URL',
            ),
            onSubmitted: (_) => _connect(),
          ),
          const SizedBox(height: 16),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: _restoreDefault,
                child: const Text('Restore Default'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: _connect,
                child: const Text('Connect & Save'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildConnectingContent() {
    final theme = Theme.of(context);

    return Padding(
      key: const ValueKey(_EditServerState.connecting),
      padding: const EdgeInsets.all(32.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const FsProgressIndicator(),
          const SizedBox(height: 24),
          Text('Connecting to server...', style: theme.textTheme.titleMedium),
          const SizedBox(height: 8),
          Text(
            _controller.text,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          TextButton(onPressed: _cancel, child: const Text('Cancel')),
        ],
      ),
    );
  }

  Widget _buildCertificateContent() {
    if (_certificateInfo == null) {
      return const SizedBox.shrink();
    }

    return TofuCertificateDialog(
      key: const ValueKey(_EditServerState.certificate),
      certificateInfo: _certificateInfo!,
      serverUrl: _controller.text,
      onAccept: _acceptCertificate,
      onReject: _rejectCertificate,
    );
  }

  Widget _buildSuccessContent() {
    final theme = Theme.of(context);

    return Padding(
      key: const ValueKey(_EditServerState.success),
      padding: const EdgeInsets.all(32.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.check_circle_outline,
            size: 64,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(height: 16),
          Text('Server saved', style: theme.textTheme.titleLarge),
          const SizedBox(height: 8),
          Text(
            _controller.text,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Done'),
          ),
        ],
      ),
    );
  }

  Widget _buildFailureContent() {
    final theme = Theme.of(context);

    return Padding(
      key: const ValueKey(_EditServerState.failure),
      padding: const EdgeInsets.all(32.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.error_outline, size: 64, color: theme.colorScheme.error),
          const SizedBox(height: 16),
          Text('Connection failed', style: theme.textTheme.titleLarge),
          const SizedBox(height: 8),
          Text(
            _errorMessage ?? 'Unknown error',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          FilledButton(onPressed: _tryAgain, child: const Text('Try Again')),
        ],
      ),
    );
  }
}
