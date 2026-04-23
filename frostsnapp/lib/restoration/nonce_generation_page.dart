import 'package:flutter/material.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:rxdart/rxdart.dart';

class NonceGenerationPage extends StatefulWidget with TitledWidget {
  final ValueStream<NonceReplenishState> stream;
  final String? deviceName;
  final Future<void> onDisconnected;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final VoidCallback onDeviceDisconnected;
  final void Function(String) onError;

  const NonceGenerationPage({
    super.key,
    required this.stream,
    this.deviceName,
    required this.onDisconnected,
    required this.onComplete,
    required this.onCancel,
    required this.onDeviceDisconnected,
    required this.onError,
  });

  @override
  State<NonceGenerationPage> createState() => _NonceGenerationPageState();

  @override
  String get titleText => 'Preparing Device';
}

class _NonceGenerationPageState extends State<NonceGenerationPage> {
  bool _handledTerminal = false;

  void _handleTerminal(VoidCallback callback) {
    if (!mounted || _handledTerminal) return;
    _handledTerminal = true;
    callback();
  }

  @override
  void initState() {
    super.initState();
    widget.onDisconnected.then((_) {
      _handleTerminal(widget.onDeviceDisconnected);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Center(
      key: const ValueKey('nonceGeneration'),
      child: NonceReplenishIndicator(
        stream: widget.stream,
        onTerminal: (terminal) {
          _handleTerminal(() {
            switch (terminal) {
              case NonceReplenishCompleted():
                widget.onComplete();
                break;
              case NonceReplenishAborted():
                widget.onError('Device disconnected during preparation');
                break;
              case NonceReplenishFailed(:final error):
                widget.onError('Failed to prepare device: $error');
                break;
            }
          });
        },
      ),
    );
  }
}
