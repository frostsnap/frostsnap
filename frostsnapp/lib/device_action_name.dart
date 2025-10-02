import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class DeviceActionNameDialogController with ChangeNotifier {
  late final FullscreenActionDialogController<void> _dialogController;
  final StreamController<void> _cancelButtonController =
      StreamController.broadcast();

  // Currently active device.
  DeviceId? get activeDeviceId => _dialogController.actionsNeeded.firstOrNull;

  DeviceActionNameDialogController() {
    _dialogController = FullscreenActionDialogController(
      title: 'Confirm device name',
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(),
      ],
      onDismissed: _onCancel,
    );
  }

  @override
  void dispose() async {
    await _cancelButtonController.close();
    _dialogController.dispose();
    super.dispose();
  }

  void _onCancel() {
    if (_cancelButtonController.isClosed) return;
    _cancelButtonController.add(null);
  }

  Future<String?> show({
    required BuildContext context,
    required DeviceId id,
    required String name,
    Function(String)? onNamed,
  }) async {
    _dialogController.addActionNeeded(context, id);

    // Check if device already has this name
    final currentName = coord.getDeviceName(id: id);
    if (currentName == name) {
      // Device already has this name, no need to rename
      await _dialogController.removeActionNeeded(id);
      onNamed?.call(name);
      return name;
    }

    await coord.finishNaming(id: id, name: name.trim());

    final confirmedName = await Stream<String?>.fromFutures([
      GlobalStreams.deviceListChangeStream
          .firstWhere((change) {
            final isRemoved = change.kind == DeviceListChangeKind.removed;
            final isNamed = change.kind == DeviceListChangeKind.named;
            return (isRemoved || isNamed) &&
                deviceIdEquals(id, change.device.id);
          })
          .then((change) => change.device.name),
      _cancelButtonController.stream.first.then((_) => null),
    ]).first.catchError((_) => null);

    await _dialogController.removeActionNeeded(id);

    if (confirmedName != null) {
      onNamed?.call(confirmedName);
    } else {
      await coord.sendCancel(id: id);
    }

    return confirmedName;
  }
}
