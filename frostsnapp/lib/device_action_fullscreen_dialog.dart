import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/theme.dart';

class FullscreenActionDialogController<T> extends ChangeNotifier {
  String? title;
  Function(BuildContext)? body;
  final Set<DeviceId> _actionNeeded = deviceIdSet([]);
  Function(BuildContext)? dismissButton;
  Function()? onDismissed;
  Future<T?>? _fut;

  FullscreenActionDialogController({
    this.title,
    this.body,
    this.dismissButton,
    this.onDismissed,
  });

  void addActionNeeded(BuildContext context, DeviceId deviceId) {
    final hadActionsNeeded = _actionNeeded.isNotEmpty;
    _actionNeeded.add(deviceId);
    if (!hadActionsNeeded) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _fut = showFullscreenActionDialog(context, controller: this);
      });
    }
  }

  Future<T?> removeActionNeeded(DeviceId deviceId) async {
    if (_actionNeeded.remove(deviceId)) _safeNotify();
    final fut = _fut;
    if (_actionNeeded.isEmpty && fut != null) {
      return await fut;
    }
    return null;
  }

  void clearAllActionsNeeded() {
    if (_actionNeeded.isEmpty) return;
    _actionNeeded.clear();
    _safeNotify();
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
  // Use
  final res = await showGeneralDialog<T>(
    context: context,
    barrierDismissible: false,
    barrierColor: Colors.black54, // same as default
    barrierLabel: '', // for accessibility
    transitionDuration: const Duration(milliseconds: 500),
    pageBuilder: (ctx, animation, secondary) {
      // build your exact same dialog tree here
      final dialog = ListenableBuilder(
        listenable: controller,
        builder: (ctx, _) {
          if (!controller.hasActionsNeeded) {
            Navigator.pop(ctx);
          }
          final theme = Theme.of(ctx);
          return Dialog.fullscreen(
            backgroundColor: theme.colorScheme.surface.withAlpha(200),
            child: Scaffold(
              backgroundColor: Colors.transparent,
              appBar: AppBar(
                elevation: 0,
                forceMaterialTransparency: true,
                automaticallyImplyLeading: false,
                leading: controller.onDismissed != null
                    ? IconButton(
                        icon: Icon(
                          Icons.close,
                          color: theme.colorScheme.onSurface,
                        ),
                        onPressed: () => Navigator.pop(ctx),
                      )
                    : null,
              ),
              body: Padding(
                padding: EdgeInsets.all(20),
                child: Center(
                  child: ConstrainedBox(
                    constraints: BoxConstraints(maxWidth: 580),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Spacer(flex: 9),
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
                        if (controller.title != null)
                          Text(
                            controller.title!,
                            style: theme.textTheme.headlineSmall,
                            textAlign: TextAlign.center,
                          ),
                        SizedBox(height: 20),
                        if (controller.body != null)
                          DefaultTextStyle(
                            style: theme.textTheme.bodyLarge!,
                            child: controller.body!.call(ctx),
                          ),
                        Spacer(flex: 3),
                        if (controller.dismissButton == null) ...[
                          Spacer(flex: 3),
                          Text(
                            'complete the action on the device or unplug it',
                            style: theme.textTheme.labelLarge?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                            textAlign: TextAlign.center,
                          ),
                        ],
                        if (controller.dismissButton != null) ...[
                          Center(child: controller.dismissButton!(context)),
                          Spacer(flex: 3),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      );

      return PopScope(
        canPop: controller.onDismissed != null,
        onPopInvokedWithResult: (didPop, result) {
          if (didPop) return;
          showCannotDismissDialog(context);
        },
        child: BackdropFilter(filter: blurFilter, child: dialog),
      );
    },
    transitionBuilder: (ctx, animation, secondary, child) {
      // fade in from 0→1 over 1 second
      return FadeTransition(opacity: animation, child: child);
    },
  );

  controller.onDismissed?.call();

  return res;
}
