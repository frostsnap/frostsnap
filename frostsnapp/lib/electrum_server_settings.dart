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
              final (network, url) = record;
              return Column(
                children: [
                  if (network.isMainnet() || developerMode) ...[
                    SizedBox(height: 10),
                    ElectrumServerSettingWidget(
                      network: network,
                      initialUrl: url,
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

  const ElectrumServerSettingWidget({
    super.key,
    required this.network,
    required this.initialUrl,
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
    setState(() {
      _controller.text = _originalUrl;
    });
  }

  void _restoreDefault() {
    setState(() {
      _controller.text = widget.network.defaultElectrumServer();
    });
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              widget.network.name(),
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
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                ElevatedButton(
                  onPressed: _isTestingConnection ? null : _confirmServerUrl,
                  child: Text("Connect & Save"),
                ),
                SizedBox(width: 8),
                ElevatedButton(
                  onPressed: _isTestingConnection ? null : _restoreDefault,
                  child: Text("Restore Default"),
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
