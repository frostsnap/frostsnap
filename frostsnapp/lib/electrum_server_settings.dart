import 'package:flutter/material.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/progress_indicator.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
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
              final url = record.url;
              final backupUrl = record.backupUrl;
              return Column(
                children: [
                  if (network.isMainnet() || developerMode) ...[
                    SizedBox(height: 10),
                    Card(
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          vertical: 8.0,
                          horizontal: 12.0,
                        ),
                        child: Column(
                          children: [
                            ElectrumServerSettingWidget(
                              network: network,
                              initialUrl: url,
                              isBackup: false,
                            ),
                            ElectrumServerSettingWidget(
                              network: network,
                              initialUrl: backupUrl,
                              isBackup: true,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ],
              );
            }).toList(),
          );
        },
      ),
    );
  }
}

class ElectrumServerSettingWidget extends StatefulWidget {
  final BitcoinNetwork network;
  final String initialUrl;
  final bool isBackup;

  const ElectrumServerSettingWidget({
    super.key,
    required this.network,
    required this.initialUrl,
    required this.isBackup,
  });

  @override
  State<ElectrumServerSettingWidget> createState() =>
      _ElectrumServerSettingWidgetState();
}

class _ElectrumServerSettingWidgetState
    extends State<ElectrumServerSettingWidget> {
  late TextEditingController _controller;
  late String _originalUrl;
  bool _isTestingConnection = false;

  @override
  void initState() {
    super.initState();
    _originalUrl = widget.initialUrl;
    _controller = TextEditingController(text: _originalUrl);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _confirmServerUrl() async {
    setState(() {
      _isTestingConnection = true;
    });
    String? error;

    try {
      final settingsCtx = SettingsContext.of(context)!;
      await settingsCtx.settings.checkAndSetElectrumServer(
        network: widget.network,
        url: _controller.text,
        isBackup: widget.isBackup,
      );
    } catch (e) {
      error = e.toString();
    }

    setState(() {
      _isTestingConnection = false;
      if (error == null) {
        _originalUrl = _controller.text;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Connection successful! Electrum server saved.'),
          ),
        );
      } else {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            backgroundColor: Theme.of(context).colorScheme.error,
            content: Text(
              'Failed to connect. Please check the server URL. ERROR: $error',
            ),
          ),
        );
      }
    });
  }

  void _resetToOriginal() {
    setState(() => _controller.text = _originalUrl);
  }

  void _restoreDefault() {
    if (widget.isBackup) {
      var defaultBackupServer = widget.network.defaultBackupElectrumServer();
      setState(() => _controller.text = defaultBackupServer);
    } else {
      var defaultElectrumServer = widget.network.defaultElectrumServer();
      setState(() => _controller.text = defaultElectrumServer);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              widget.isBackup
                  ? '${widget.network.name()} (backup)'
                  : widget.network.name(),
              style: Theme.of(context).textTheme.titleMedium,
            ),
            SizedBox(height: 8),
            TextField(
              controller: _controller,
              decoration: InputDecoration(
                border: OutlineInputBorder(),
                suffixIcon: _controller.text != _originalUrl
                    ? IconButton(
                        icon: Icon(Icons.undo),
                        onPressed: _resetToOriginal,
                      )
                    : null,
              ),
            ),
            SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              spacing: 8,
              children: [
                TextButton(
                  onPressed: _isTestingConnection ? null : _restoreDefault,
                  child: Text("Restore Default"),
                ),
                FilledButton(
                  onPressed: _isTestingConnection ? null : _confirmServerUrl,
                  child: Text("Connect & Save"),
                ),
              ],
            ),
            SizedBox(height: 8),
          ],
        ),
        if (_isTestingConnection)
          Positioned.fill(
            child: Container(
              color: Colors.black.withValues(alpha: 127),
              child: Center(child: FsProgressIndicator()),
            ),
          ),
      ],
    );
  }
}
