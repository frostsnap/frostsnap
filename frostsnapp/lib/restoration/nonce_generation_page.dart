import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';

class NonceGenerationPage extends StatefulWidget with TitledWidget {
  final Stream<NonceReplenishState> stream;
  final String? deviceName;
  final Future<void> onDisconnected;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final VoidCallback onDeviceDisconnected;
  final Function(String) onError;

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
  bool _hasCompleted = false;
  bool _hasErrored = false;
  StreamSubscription? _streamSubscription;

  @override
  void initState() {
    super.initState();
    widget.onDisconnected.then((_) {
      if (mounted && !_hasCompleted) {
        widget.onDeviceDisconnected();
      }
    });
  }

  @override
  void dispose() {
    _streamSubscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<NonceReplenishState>(
      stream: widget.stream,
      builder: (context, snapshot) {
        if (snapshot.hasError && !_hasErrored) {
          _hasErrored = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) {
              widget.onError('Failed to prepare device: ${snapshot.error}');
            }
          });
        }

        final state = snapshot.data;

        if (state != null && !_hasCompleted && !_hasErrored) {
          final isComplete = state.isFinished();
          if (isComplete) {
            _hasCompleted = true;
            Future.delayed(Durations.long1, () {
              if (mounted) {
                widget.onComplete();
              }
            });
          }
        }

        if (state?.abort == true && !_hasCompleted && !_hasErrored) {
          _hasErrored = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) {
              widget.onError('Device disconnected during preparation');
            }
          });
        }

        return Column(
          key: const ValueKey('nonceGeneration'),
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              constraints: BoxConstraints(minHeight: 120),
              child: MinimalNonceReplenishWidget(
                stream: widget.stream,
                autoAdvance: false,
              ),
            ),
          ],
        );
      },
    );
  }
}
