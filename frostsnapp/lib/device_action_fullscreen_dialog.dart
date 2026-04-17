import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/theme.dart';

const String deviceSvg = '''
<svg width="34.986689421321785mm" height="42.99022935571762mm" viewBox="0 0 34.986689421321785 42.99022935571762" xmlns="http://www.w3.org/2000/svg" version="1.1">
<g id="Binder022" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder022_f0000"  d="M -14.964990406185642 18.586987365833934 C -12.222647326109056 21.262466115746093 -6.102466677437631 21.177514052687993 0.0 21.19999999999999 C 6.121690939255835 21.173197742531492 12.231613033666312 21.274073642081408 14.969082389611895 18.60771394747341 C 17.192036356405605 16.42026948598908 17.145133620721403 12.729626455124311 17.150337936776445 9.214654998137103 C 17.15016857520842 9.062541610979817 17.149999999999988 8.910928801845651 17.15 8.759999999999998 L 17.15 -8.760000000000002 C 17.15 -8.914753765468582 17.150169136378064 -9.069446409953057 17.150338175634953 -9.2239777310766 C 17.142579739222196 -12.72727276368968 17.19842080113543 -16.18678348818877 15.049795021706236 -18.379036431181923 C 12.355533216618058 -21.090033156964946 6.171452934273059 -21.04122596621769 0.0023556717517863035 -21.0999930634531 C -6.101080496066673 -21.113188247010854 -12.221201449009136 -21.235110995509284 -14.9651450300972 -18.57374603173769 C -17.20075798684543 -16.36281066897254 -17.14189748249141 -12.78040602545065 -17.150337727621697 -9.223998902144816 C -17.1501688375326 -9.069320114984011 -17.14999999999999 -8.914616780668991 -17.15 -8.76 L -17.15 8.76 C -17.149999999999984 8.914615841852937 -17.150168835482276 9.06931823718443 -17.150337724548145 9.223997026763582 C -17.14207388097222 12.781031521903902 -17.20025878454716 16.36447394034848 -14.964990406185638 18.586987365833927 Z M -14.99999999999999 15.099999999999996 A 4 4 0 0 0 -11 19.1L 10.999999999999982 19.099999999999994 A 4 4 0 0 0 15 15.1L 14.999999999999991 -11.900000000000004 A 4 4 0 0 0 11 -15.9L -10.999999999999982 -15.900000000000002 A 4 4 0 0 0 -15 -11.9L -14.999999999999991 15.099999999999996 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.3;fill-rule: evenodd"/>
<title>b'Binder022'</title>
</g>
<g id="Binder023" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder023_f0000"  d="M -10.999999999999979 19.099999999999998 A 4 4 0 0 1 -15 15.1L -15.0 -11.9 A 4 4 0 0 1 -11 -15.9L 10.999999999999982 -15.900000000000002 A 4 4 0 0 1 15 -11.9L 15.0 15.099999999999996 A 4 4 0 0 1 11 19.1L -10.99999999999998 19.099999999999998 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.1;fill-rule: evenodd"/>
<title>b'Binder023'</title>
</g>
</svg>
''';

class FullscreenActionDialogController<T> extends ChangeNotifier {
  String? title;
  Function(BuildContext)? body;
  List<Widget>? actionButtons;
  Function()? onDismissed;

  /// The set of devices this controller is tracking. Invariant: seeded at
  /// construction and only ever shrinks. Use `remove*` methods to mark a
  /// device as "done"; the dialog auto-dismisses when the set is empty or
  /// when none of its devices remain connected.
  final Set<DeviceId> _actionNeeded;
  Set<DeviceId> _connectedDevices;
  NavigatorState? _navigator;
  bool _isShowing = false;
  bool _enabled = true;
  Future<T?>? _currentDialogFuture;
  StreamSubscription? _deviceListSubscription;

  /// Construct the controller with the full set of devices it should track.
  ///
  /// - [context] is used once to resolve the root navigator (the dialog is
  ///   always pushed onto it).
  /// - [devices] is the invariant action-needed set: it's seeded here and
  ///   only ever shrinks via the `remove*` methods. We also seed
  ///   `_connectedDevices` with the same set so `shouldShow` is `true`
  ///   immediately — callers pass in devices they've just observed to be
  ///   connected, so this is always correct at the moment of construction.
  ///   The real device list then takes over via the stream subscription.
  FullscreenActionDialogController({
    required BuildContext context,
    required Iterable<DeviceId> devices,
    this.title,
    this.body,
    this.actionButtons,
    this.onDismissed,
  }) : _actionNeeded = deviceIdSet(devices),
       _connectedDevices = deviceIdSet(devices) {
    _navigator = Navigator.of(context, rootNavigator: true);
    _deviceListSubscription = GlobalStreams.deviceListSubject.listen((update) {
      _connectedDevices = deviceIdSet(
        update.state.devices.map((dev) => dev.id).toList(),
      );
      _reconcile();
    });
    // The action-needed set is non-empty and all devices are (believed to
    // be) connected, so this kicks off the first `_show()` synchronously.
    _reconcile();
  }

  Future<T?> removeActionNeeded(DeviceId deviceId) async {
    if (_actionNeeded.remove(deviceId)) _reconcile();
    return _actionNeeded.isEmpty ? await _awaitDismiss() : null;
  }

  Future<T?> batchRemoveActionNeeded(Iterable<DeviceId> deviceIds) async {
    bool didRemove = false;
    for (final id in deviceIds) didRemove |= _actionNeeded.remove(id);
    if (didRemove) _reconcile();
    return _actionNeeded.isEmpty ? await _awaitDismiss() : null;
  }

  Future<T?> clearAllActionsNeeded() async {
    if (_actionNeeded.isEmpty) return null;
    _actionNeeded.clear();
    _reconcile();
    return await _awaitDismiss();
  }

  Future<T?> clearAllExcept(Iterable<DeviceId> deviceIds) async {
    final keep = deviceIdSet(deviceIds);
    final before = _actionNeeded.length;
    _actionNeeded.retainWhere(keep.contains);
    if (_actionNeeded.length != before) _reconcile();
    return _actionNeeded.isEmpty ? await _awaitDismiss() : null;
  }

  Iterable<DeviceId> get actionsNeeded => _actionNeeded;

  /// Master switch. When false, the dialog is hidden regardless of
  /// `_actionNeeded` and will not show on future reconciles. Flip it back to
  /// true to let the controller drive visibility from device state again.
  bool get enabled => _enabled;
  set enabled(bool value) {
    if (_enabled == value) return;
    _enabled = value;
    _reconcile();
  }

  /// True iff the controller is enabled and at least one action-needed device
  /// is currently connected. This is the sole visibility condition.
  bool get shouldShow =>
      _enabled && _actionNeeded.any(_connectedDevices.contains);

  void _reconcile() {
    if (shouldShow && !_isShowing) _show();
    // Hide is handled by the dialog's ListenableBuilder, which pops itself
    // when `shouldShow` flips to false — the pop must happen inside the
    // dialog's own Navigator scope.
    if (hasListeners) notifyListeners();
  }

  void _show() {
    // Use the overlay's context (a descendant of the Navigator) rather than
    // the NavigatorState's own element — showGeneralDialog walks up from the
    // given context to find a Navigator ancestor, and the navigator's own
    // element isn't an ancestor of itself.
    final overlayContext = _navigator?.overlay?.context;
    if (overlayContext == null) {
      // Navigator's overlay isn't mounted yet — retry after the current
      // frame so we don't drop the "show" on the floor.
      WidgetsBinding.instance.addPostFrameCallback((_) => _reconcile());
      return;
    }
    _isShowing = true;
    final future = showFullscreenActionDialog<T>(
      overlayContext,
      controller: this,
    );
    _currentDialogFuture = future;
    future.then((_) {
      _isShowing = false;
      _currentDialogFuture = null;
      // Re-check in case a reconnect happened while the pop was animating.
      _reconcile();
    });
  }

  Future<T?> _awaitDismiss() async =>
      await (_currentDialogFuture ?? Future<T?>.value(null));

  /// Awaits the dialog's natural dismissal — whether that came from the
  /// action-needed set emptying, all action-needed devices disconnecting
  /// (triggering `shouldShow`→false), or a programmatic dispose. Use this
  /// when the caller just wants to block on "user-facing dialog is gone"
  /// rather than triggering a specific state transition.
  Future<T?> awaitDismissed() => _awaitDismiss();

  @override
  void dispose() {
    _deviceListSubscription?.cancel();
    _actionNeeded.clear();
    if (hasListeners) notifyListeners();
    super.dispose();
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

  final bodyColumn = Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      SvgPicture.string(
        deviceSvg,
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
    ],
  );

  final footerRow = Row(
    mainAxisAlignment: MainAxisAlignment.spaceBetween,
    mainAxisSize: MainAxisSize.max,
    spacing: 8,
    children: actionButtons ?? [],
  );

  final card = Card(
    color: Colors.black,
    margin: EdgeInsets.zero,
    child: Padding(
      padding: const EdgeInsets.all(20.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          bodyColumn,
          if (actionButtons != null) ...[SizedBox(height: 32), footerRow],
        ],
      ),
    ),
  );

  final scaffold = Scaffold(
    backgroundColor: Colors.black,
    body: Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16.0),
        child: bodyColumn,
      ),
    ),
    persistentFooterButtons: [footerRow],
  );

  // Latched so we pop exactly once. Subsequent notifies while the pop
  // transition is animating (device list churn, verify-stream updates, the
  // controller's own post-dismiss reconcile) must not trigger another pop —
  // otherwise we'd chew up whatever route is underneath.
  var popScheduled = false;
  final listenableBuilder = ListenableBuilder(
    listenable: controller,
    builder: (context, _) {
      if (!controller.shouldShow && !popScheduled) {
        popScheduled = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (Navigator.of(context).canPop()) Navigator.of(context).pop();
        });
      }
      final isCompact =
          WindowSizeContext.of(context) == WindowSizeClass.compact;
      return isCompact ? scaffold : card;
    },
  );

  final res = await MaybeFullscreenDialog.show(
    context: context,
    backgroundColor: Colors.transparent,
    blurCompactBackground: true,
    animationDuration: Durations.medium4,
    animationIsFade: true,
    child: PopScope(
      canPop: controller.onDismissed != null,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        showCannotDismissDialog(context);
      },
      child: listenableBuilder,
    ),
  );

  controller.onDismissed?.call();

  return res;
}

class DeviceActionHint extends StatelessWidget {
  final String label;
  final IconData icon;

  const DeviceActionHint({
    this.label = 'Confirm on device',
    this.icon = Icons.touch_app_rounded,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          label,
          style: theme.textTheme.labelMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4.0),
          child: Icon(
            icon,
            color: theme.colorScheme.onSurfaceVariant,
            size: 20,
          ),
        ),
      ],
    );
  }
}

class InfoRow {
  String label;
  String body;

  InfoRow(this.label, this.body);

  Widget toWidget(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      spacing: 12,
      children: [
        Expanded(
          flex: 2,
          child: Text(
            label,
            textAlign: TextAlign.end,
            style: TextStyle(color: theme.colorScheme.onSurfaceVariant),
          ),
        ),
        Expanded(flex: 3, child: Text(body, style: monospaceTextStyle)),
      ],
    );
  }

  static Widget toColumn(BuildContext context, Iterable<InfoRow> rows) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 12,
      children: rows.map((r) => r.toWidget(context)).toList(),
    );
  }
}
