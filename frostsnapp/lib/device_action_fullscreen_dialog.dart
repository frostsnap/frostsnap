import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/theme.dart';

class FullscreenActionDialogController<T> extends ChangeNotifier {
  String? title;
  Function(BuildContext)? body;
  List<Widget>? actionButtons;
  final Set<DeviceId> _actionNeeded = deviceIdSet([]);
  Function()? onDismissed;
  Future<T?> _fut = Future.value(null);

  FullscreenActionDialogController({
    this.title,
    this.body,
    this.actionButtons,
    this.onDismissed,
  });

  void addActionNeeded(BuildContext context, DeviceId deviceId) {
    final hadActionsNeeded = _actionNeeded.isNotEmpty;
    _actionNeeded.add(deviceId);
    if (hadActionsNeeded) return null;
    final completer = Completer<T?>();
    _fut = completer.future;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      completer.complete(showFullscreenActionDialog(context, controller: this));
    });
  }

  Future<T?> removeActionNeeded(DeviceId deviceId) async {
    final wasActive = _actionNeeded.isNotEmpty;
    if (wasActive) {
      if (_actionNeeded.remove(deviceId)) _safeNotify();
      if (_actionNeeded.isEmpty) return await _fut;
    }
    return null;
  }

  void batchAddActionNeeded(
    BuildContext context,
    Iterable<DeviceId> deviceIds,
  ) {
    final wasActive = _actionNeeded.isNotEmpty;
    bool didAdd = false;
    for (final id in deviceIds) didAdd |= _actionNeeded.add(id);
    if (wasActive || !didAdd) return;

    final completer = Completer<T?>();
    _fut = completer.future;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      completer.complete(showFullscreenActionDialog(context, controller: this));
    });
  }

  Future<T?> batchRemoveActionNeeded(Iterable<DeviceId> deviceIds) async {
    bool didRemove = false;
    for (final id in deviceIds) didRemove |= _actionNeeded.remove(id);
    if (didRemove && _actionNeeded.isEmpty) {
      _safeNotify();
      return await _fut;
    }
    return null;
  }

  Future<T?> clearAllActionsNeeded() async {
    final wasActive = _actionNeeded.isNotEmpty;
    if (wasActive) {
      _actionNeeded.clear();
      _safeNotify();
      return await _fut;
    }
    return null;
  }

  Future<T?> clearAllExcept(Iterable<DeviceId> deviceIds) async {
    final wasActive = _actionNeeded.isNotEmpty;
    final exceptMap = deviceIdSet(deviceIds);
    _actionNeeded.retainWhere((id) => exceptMap.contains(id));
    if (wasActive && _actionNeeded.isEmpty) {
      _safeNotify();
      return await _fut;
    }
    return null;
  }

  bool get hasActionsNeeded => _actionNeeded.isNotEmpty;
  Iterable<DeviceId> get actionsNeeded => _actionNeeded;

  @override
  void dispose() {
    _actionNeeded.clear();
    _safeNotify();
    super.dispose();
  }

  /// This is so that we can avoid triggering a rebuild of
  void _safeNotify() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (hasListeners) notifyListeners();
    });
  }
}

void showCannotDismissDialog(BuildContext context) async {
  showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: Text('Cannot dismiss'),
        content: Text('To dismiss this screen, unplug the device.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text('Okay'),
          ),
        ],
      );
    },
  );
}

Future<T?> showFullscreenActionDialog<T>(
  BuildContext context, {
  required FullscreenActionDialogController<T> controller,
}) async {
  final theme = Theme.of(context);

  final title = controller.title;
  final body = controller.body;
  final actionButtons = controller.actionButtons;

  final content = Padding(
    padding: const EdgeInsets.all(20).copyWith(top: 32),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SvgPicture.string(
          DeviceWidget.deviceSvg,
          width: 162,
          height: 134,
          colorFilter: ColorFilter.mode(
            theme.colorScheme.onSurface,
            BlendMode.srcATop,
          ),
        ),
        if (title != null) ...[
          SizedBox(height: 32),
          Text(
            title,
            style: theme.textTheme.headlineSmall,
            textAlign: TextAlign.center,
          ),
        ],
        if (body != null) ...[
          SizedBox(height: 24),
          DefaultTextStyle(
            style: theme.textTheme.bodyLarge!,
            child: body(context),
          ),
        ],
        if (actionButtons != null) ...[
          SizedBox(height: 32),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            mainAxisSize: MainAxisSize.max,
            spacing: 8,
            children: actionButtons,
          ),
        ],
      ],
    ),
  );

  final listenableBuilder = ListenableBuilder(
    listenable: controller,
    builder: (context, _) {
      if (!controller.hasActionsNeeded) {
        Navigator.pop(context);
      }
      final windowSize = WindowSizeContext.of(context);
      final isCompact = windowSize == WindowSizeClass.compact;

      return SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 580),
            child: isCompact
                ? content
                : Card.outlined(
                    color: Colors.black26,
                    margin: EdgeInsets.zero,
                    child: content,
                  ),
          ),
        ),
      );
    },
  );

  final res = await MaybeFullscreenDialog.show(
    context: context,
    backgroundColor: Colors.transparent,
    blurCompactBackground: true,
    animationDuration: Durations.medium4,
    child: PopScope(
      canPop: controller.onDismissed != null,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        showCannotDismissDialog(context);
      },
      child: BackdropFilter(filter: blurFilter, child: listenableBuilder),
    ),
  );

  controller.onDismissed?.call();

  return res;
}
