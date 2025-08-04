import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
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
  final _progressController =
      StreamController<FirmwareUpgradeState>.broadcast();

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

    _dialogController = FullscreenActionDialogController(
      title: 'Upgrade Firmware',
      body: (context) => Card(
        margin: EdgeInsets.zero,
        child: ListTile(
          title: Text('Firmware Digest'),
          subtitle: Text(
            coord.upgradeFirmwareDigest() ?? '',
            style: monospaceTextStyle,
          ),
        ),
      ),
      actionButtons: [
        StreamBuilder(
          stream: _progressController.stream,
          initialData: FirmwareUpgradeState.empty(),
          builder: (context, snapshot) => switch (snapshot.requireData.stage) {
            FirmwareUpgradeStage.Acks => OutlinedButton(
              child: Text('Cancel'),
              onPressed: () async => await coord.cancelProtocol(),
            ),
            FirmwareUpgradeStage.Progress => SizedBox.shrink(),
          },
        ),
        StreamBuilder(
          stream: _progressController.stream,
          initialData: FirmwareUpgradeState.empty(),
          builder: (context, snapshot) {
            final state = snapshot.requireData;
            final theme = Theme.of(context);
            return Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
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
                    SizedBox(
                      width: 100,
                      child: LinearProgressIndicator(value: state.progress),
                    ),
                  ],
                },
              ],
            );
          },
        ),
      ],
      onDismissed: () async => await coord.cancelProtocol(),
    );
  }

  @override
  dispose() {
    _sub.cancel();
    _dialogController.dispose();
    _progressController.close();
    super.dispose();
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

    await _dialogController.clearAllActionsNeeded();
    return progress == 1.0;
  }
}
