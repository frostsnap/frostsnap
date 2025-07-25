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
  Future<T?>? _fut;

  FullscreenActionDialogController({
    this.title,
    this.body,
    this.actionButtons,
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

  final res2 = await MaybeFullscreenDialog.show(
    context: context,
    backgroundColor: Colors.transparent,
    blurCompactBackground: true,
    child: PopScope(
      canPop: controller.onDismissed != null,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        showCannotDismissDialog(context);
      },
      child: BackdropFilter(filter: blurFilter, child: listenableBuilder),
    ),
  );

  // final res = await showDialog<T>(
  //   context: context,
  //   barrierDismissible: false,
  //   builder: (ctx) {
  //     final dialog = ListenableBuilder(
  //       listenable: controller,
  //       builder: (ctx, _) {
  //         if (!controller.hasActionsNeeded) {
  //           Navigator.pop(ctx);
  //         }
  //         final windowSize = WindowSizeContext.of(context);
  //         final isCompact = windowSize == WindowSizeClass.compact;
  //         return Dialog.fullscreen(
  //           backgroundColor: Colors.transparent,
  //           child: SafeArea(
  //             child: Center(
  //               child: ConstrainedBox(
  //                 constraints: BoxConstraints(maxWidth: 580),
  //                 child: Padding(
  //                   padding: EdgeInsets.all(16),
  //                   child: isCompact
  //                       ? content
  //                       : Card.outlined(
  //                           child: Padding(
  //                             padding: EdgeInsets.all(16),
  //                             child: content,
  //                           ),
  //                         ),
  //                 ),
  //               ),
  //             ),
  //           ),
  //         );
  //       },
  //     );

  //     return PopScope(
  //       canPop: controller.onDismissed != null,
  //       onPopInvokedWithResult: (didPop, result) {
  //         if (didPop) return;
  //         showCannotDismissDialog(context);
  //       },
  //       child: BackdropFilter(filter: blurFilter, child: dialog),
  //     );
  //   },
  // );

  controller.onDismissed?.call();

  return res2;
}
