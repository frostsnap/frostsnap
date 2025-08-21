import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_create.dart';

enum FirmwareUpgradeStage { Acks, Progress }

class FirmwareUpgradeState {
  final FirmwareUpgradeStage stage;
  final int? neededAcks;
  final int? acks;
  final double? progress;

  const FirmwareUpgradeState.empty()
    : stage = FirmwareUpgradeStage.Acks,
      neededAcks = null,
      acks = null,
      progress = null;

  const FirmwareUpgradeState.acks({required int neededAcks, required int acks})
    : stage = FirmwareUpgradeStage.Acks,
      progress = null,
      neededAcks = neededAcks,
      acks = acks;

  const FirmwareUpgradeState.progress({required double progress})
    : stage = FirmwareUpgradeStage.Progress,
      progress = progress,
      neededAcks = null,
      acks = null;

  @override
  bool operator ==(Object o) =>
      o is FirmwareUpgradeState &&
      o.stage == stage &&
      o.neededAcks == neededAcks &&
      o.acks == acks &&
      o.progress == progress;
}

class DeviceActionUpgradeController with ChangeNotifier {
  late final StreamSubscription<DeviceListUpdate> _sub;
  late final FullscreenActionDialogController<void> _dialogController;
  int _needsUpgradeCount = 0;
  final _progressController = StreamController<FirmwareUpgradeState>();

  DeviceActionUpgradeController() {
    _sub = GlobalStreams.deviceListSubject.listen((update) {
      final count = update.state.devices
          .where((dev) => dev.needsFirmwareUpgrade())
          .length;
      if (count != _needsUpgradeCount) {
        _needsUpgradeCount = count;
        notifyListeners();
      }
    });
    // Ensure that we do not skip device events.
    final replayStream = _progressController.stream.toReplaySubject();

    _dialogController = FullscreenActionDialogController(
      title: 'Upgrade Firmware',
      body: (context) => Card(
        margin: EdgeInsets.zero,
        child: ListTile(
          title: Text('New Firmware Digest'),
          subtitle: Text(
            coord.upgradeFirmwareDigest() ?? '',
            style: monospaceTextStyle,
          ),
        ),
      ),
      actionButtons: [
        StreamBuilder(
          stream: replayStream,
          initialData: FirmwareUpgradeState.empty(),
          builder: (context, snapshot) => switch (snapshot.requireData.stage) {
            FirmwareUpgradeStage.Acks => OutlinedButton(
              child: Text('Cancel'),
              onPressed: _onCancel,
            ),
            FirmwareUpgradeStage.Progress => SizedBox.shrink(),
          },
        ),
        StreamBuilder(
          stream: replayStream,
          initialData: FirmwareUpgradeState.empty(),
          builder: (context, snapshot) {
            final state = snapshot.requireData;
            final theme = Theme.of(context);
            return Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 4,
              children: [
                ...switch (state.stage) {
                  FirmwareUpgradeStage.Acks => [
                    Text(
                      'Confirm on device',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    LargeCircularProgressIndicator(
                      size: 36,
                      progress: state.acks ?? 0,
                      total: state.neededAcks ?? 1,
                    ),
                  ],
                  FirmwareUpgradeStage.Progress => [
                    Text(
                      'Upgrading...',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.symmetric(
                        vertical: 16.0,
                        horizontal: 8.0,
                      ),
                      child: SizedBox(
                        width: 100,
                        child: LinearProgressIndicator(value: state.progress),
                      ),
                    ),
                  ],
                },
              ],
            );
          },
        ),
      ],

      // NOTE: We purposefully disable cancelling on 'back' as there is no way to stop a commencing
      // upgrade other than unplugging the device.
      // onDismissed: _onCancel,
    );
  }

  @override
  dispose() {
    _sub.cancel();
    _dialogController.dispose();
    _progressController.close();
    super.dispose();
  }

  void _onCancel() async {
    await coord.cancelProtocol();
  }

  /// Number of devices that needs upgrade.
  int get count => _needsUpgradeCount;

  /// Upgrade progress stream.
  Stream<FirmwareUpgradeState> get progressStream => _progressController.stream;

  /// Starts the upgrade device firmware flow.
  Future<bool> run(BuildContext context) async {
    _progressController.add(FirmwareUpgradeState.empty());

    await for (final state in coord.startFirmwareUpgrade()) {
      if (!context.mounted) {
        await coord.cancelProtocol();
        await _dialogController.clearAllActionsNeeded();
        return false;
      }
      _progressController.add(
        FirmwareUpgradeState.acks(
          neededAcks: state.needUpgrade.length,
          acks: state.confirmations.length,
        ),
      );
      for (final id in state.needUpgrade) {
        _dialogController.addActionNeeded(context, id);
      }
      if (state.abort) {
        await _dialogController.clearAllActionsNeeded();
        return false;
      }
      if (state.upgradeReadyToStart) {
        break;
      }
    }

    var progress = 0.0;
    await for (progress in coord.enterFirmwareUpgradeMode()) {
      _progressController.add(
        FirmwareUpgradeState.progress(progress: progress),
      );
    }
    final success = progress == 1.0;
    await Future.delayed(Duration(seconds: 1));
    await _dialogController.clearAllActionsNeeded();
    if (context.mounted) await showUpgradeDoneDialog(context, success);
    return success;
  }

  Future<void> showUpgradeDoneDialog(BuildContext context, bool success) async {
    await showDialog(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: Text(success ? 'Upgrade Successful' : 'Upgrade Failed'),
          content: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 560, minWidth: 280),
            child: success
                ? Card(
                    margin: EdgeInsets.zero,
                    child: ListTile(
                      title: Text('Upgraded to Latest Firmware'),
                      subtitle: Text(
                        coord.upgradeFirmwareDigest() ?? '',
                        style: monospaceTextStyle,
                      ),
                    ),
                  )
                : Text(
                    'Either a device was disconnected mid-upgrade, or you have encountered a bug!',
                  ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text('Done'),
            ),
          ],
          actionsAlignment: MainAxisAlignment.end,
        );
      },
    );
  }
}
