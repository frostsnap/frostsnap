import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/theme.dart';

class FullscreenActionDialogController extends ChangeNotifier {
  String? _title;
  String? _subtitle;
  final Set<DeviceId> _actionNeeded = deviceIdSet([]);

  FullscreenActionDialogController({String? title, String? subtitle})
    : _title = title,
      _subtitle = subtitle;

  setContent(String title, String subtitle) {
    if (title == _title && subtitle == _subtitle) return;
    _title = title;
    _subtitle = subtitle;
    _safeNotify();
  }

  addActionNeeded(BuildContext context, DeviceId deviceId) {
    final hadActionsNeeded = _actionNeeded.isNotEmpty;
    _actionNeeded.add(deviceId);
    if (!hadActionsNeeded) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => showFullscreenActionDialog(context, controller: this),
      );
    }
  }

  removeActionNeeded(DeviceId deviceId) {
    if (_actionNeeded.remove(deviceId)) _safeNotify();
  }

  String? get title => _title;
  String? get subtitle => _subtitle;
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

Widget buildFullscreenActionDialog(
  BuildContext context, {
  String? title,
  String? subtitle,
}) {
  final theme = Theme.of(context);
  return Dialog.fullscreen(
    backgroundColor: theme.colorScheme.surface.withAlpha(200),
    child: Padding(
      padding: EdgeInsets.all(20),
      child: Column(
        mainAxisSize: MainAxisSize.max,
        spacing: 20,
        children: [
          Expanded(
            child: Center(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxWidth: 580),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
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
                    SizedBox(height: 32),
                    if (title != null)
                      Text(
                        title,
                        style: theme.textTheme.headlineSmall,
                        textAlign: TextAlign.center,
                      ),
                    SizedBox(height: 20),
                    if (subtitle != null)
                      Text(
                        subtitle,
                        style: theme.textTheme.bodyLarge,
                        textAlign: TextAlign.center,
                      ),
                  ],
                ),
              ),
            ),
          ),
          Text(
            'To dismiss this screen, unplug the device.',
            style: theme.textTheme.labelLarge?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
            textAlign: TextAlign.center,
          ),
        ],
      ),
    ),
  );
}

void showFullscreenActionDialog(
  BuildContext context, {
  required FullscreenActionDialogController controller,
}) async {
  await showDialog(
    context: context,
    barrierDismissible: false,
    builder: (context) {
      final dialog = ListenableBuilder(
        listenable: controller,
        builder: (context, _) {
          if (!controller.hasActionsNeeded) {
            Navigator.pop(context);
          }
          return buildFullscreenActionDialog(
            context,
            title: controller.title,
            subtitle: controller.subtitle,
          );
        },
      );

      return PopScope(
        canPop: false,
        onPopInvokedWithResult: (didPop, result) {
          if (didPop) return;
          showCannotDismissDialog(context);
        },
        child: BackdropFilter(filter: blurFilter, child: dialog),
      );
    },
  );
}
