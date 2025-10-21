import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/restoration/material_dialog_card.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

class PlugInPromptView extends StatefulWidget with TitledWidget {
  final RecoveryContext context;
  final void Function(RecoverShare candidate) onCandidateDetected;

  const PlugInPromptView({
    super.key,
    required this.context,
    required this.onCandidateDetected,
  });

  @override
  String get titleText => 'Restore with existing device';

  @override
  State<PlugInPromptView> createState() => _PlugInPromptViewState();
}

class _PlugInPromptViewState extends State<PlugInPromptView> {
  late StreamSubscription _subscription;
  bool blankDeviceInserted = false;

  @override
  void initState() {
    super.initState();

    _subscription = coord.waitForRecoveryShare().listen((
      waitForRecoverShareState,
    ) async {
      blankDeviceInserted = false;

      if (waitForRecoverShareState.shares.isNotEmpty) {
        final detectedShare = waitForRecoverShareState.shares.first;
        setState(() {
          widget.onCandidateDetected(detectedShare);
        });
      } else {
        setState(() {
          blankDeviceInserted = waitForRecoverShareState.blank.isNotEmpty;
        });
      }
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    Widget displayWidget;
    if (blankDeviceInserted) {
      displayWidget = MaterialDialogCard(
        key: const ValueKey('warning-blank'),
        backgroundColor: theme.colorScheme.surfaceContainerLow,
        iconData: Icons.warning_amber_rounded,
        title: Text('Empty Device'),
        content: Text(
          'The device you plugged in has no key on it.',
          textAlign: TextAlign.center,
        ),
        actions: [],
      );
    } else {
      displayWidget = Semantics(
        label: 'Waiting for device to connect',
        child: CircularProgressIndicator(),
      );
    }

    final String prompt = switch (widget.context) {
      ContinuingRestorationContext(:final restorationId) => () {
        final name = coord
            .getRestorationState(restorationId: restorationId)!
            .keyName;
        return 'Plug in a Frostsnap to continue restoring "$name".';
      }(),
      AddingToWalletContext(:final accessStructureRef) => () {
        final name = coord
            .getFrostKey(keyId: accessStructureRef.keyId)!
            .keyName();
        return 'Plug in a Frostsnap to add it to "$name".';
      }(),
      NewRestorationContext() =>
        'Plug in your Frostsnap device\nto begin wallet restoration.',
    };

    return Column(
      key: const ValueKey('plugInPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        MaterialDialogCard(
          iconData: Icons.usb_rounded,
          title: Text('Waiting for device'),
          content: Text(prompt, textAlign: TextAlign.center),
          actions: [
            AnimatedSize(
              duration: Durations.short4,
              curve: Curves.easeInOutCubicEmphasized,
              child: displayWidget,
            ),
          ],
          actionsAlignment: MainAxisAlignment.center,
        ),
      ],
    );
  }
}
