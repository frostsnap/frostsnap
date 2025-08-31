import 'dart:ui';
import 'package:flutter/material.dart';
import 'theme.dart';

enum WindowSizeClass {
  compact(maxWidth: 600),
  medium(maxWidth: 840),
  expanded(maxWidth: 1200);

  const WindowSizeClass({required this.maxWidth});

  static WindowSizeClass fromWidth(double width) {
    if (width < 600) {
      return WindowSizeClass.compact;
    }
    if (width < 840) {
      return WindowSizeClass.medium;
    }
    return WindowSizeClass.expanded;
  }

  /// Max width (exclusive).
  final double maxWidth;
}

class WindowSizeContext extends InheritedWidget {
  final WindowSizeClass windowSizeClass;

  const WindowSizeContext({
    super.key,
    required this.windowSizeClass,
    required super.child,
  });

  static WindowSizeClass of(BuildContext context) {
    Size size(BuildContext context) {
      final view = View.of(context);
      return view.physicalSize / view.devicePixelRatio;
    }

    return context
            .dependOnInheritedWidgetOfExactType<WindowSizeContext>()
            ?.windowSizeClass ??
        WindowSizeClass.fromWidth(size(context).width);
  }

  @override
  bool updateShouldNotify(covariant InheritedWidget oldWidget) {
    return false;
  }
}

class MaybeFullscreenDialog extends StatefulWidget {
  final Widget? child;
  final Color? backgroundColor;
  final bool blurCompactBackground;
  const MaybeFullscreenDialog({
    super.key,
    this.child,
    this.backgroundColor,
    this.blurCompactBackground = false,
  });

  static Future<T?> show<T>({
    required BuildContext context,
    bool barrierDismissible = false,
    bool blurCompactBackground = false,
    Duration? animationDuration,
    bool animationIsFade = false,
    Color? backgroundColor,
    Widget? child,
  }) {
    return showGeneralDialog(
      context: context,
      barrierDismissible: barrierDismissible,
      barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
      pageBuilder: (context, animation, secondaryAnimation) =>
          MaybeFullscreenDialog(
            blurCompactBackground: blurCompactBackground,
            backgroundColor:
                backgroundColor ?? Theme.of(context).colorScheme.surface,
            child: child,
          ),
      transitionDuration: Durations.medium2,
      transitionBuilder: (context, animation, secondaryAnimation, child) {
        if (animationIsFade)
          return FadeTransition(opacity: animation, child: child);

        final curvedAnimation = CurvedAnimation(
          parent: animation,
          curve: Curves.easeInOutCubicEmphasized,
        );

        return SlideTransition(
          position: Tween<Offset>(
            begin: const Offset(1, 0),
            end: Offset.zero,
          ).animate(curvedAnimation),
          child: FadeTransition(opacity: animation, child: child),
        );
      },
    );
  }

  @override
  State<MaybeFullscreenDialog> createState() => _MaybeFullscreenDialogState();
}

class _MaybeFullscreenDialogState extends State<MaybeFullscreenDialog>
    with WidgetsBindingObserver {
  late final ValueNotifier<WindowSizeClass> _sizeClass;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _sizeClass = ValueNotifier(
      WindowSizeClass.fromWidth(getWindowSize().width),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _sizeClass.dispose();
    super.dispose();
  }

  @override
  void didChangeMetrics() {
    super.didChangeMetrics();
    _sizeClass.value = WindowSizeClass.fromWidth(getWindowSize().width);
  }

  Size getWindowSize() {
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    return view.physicalSize / view.devicePixelRatio;
  }

  final boxKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: _sizeClass,
      child: ConstrainedBox(
        key: boxKey,
        constraints: const BoxConstraints(maxWidth: 560),
        child: widget.child,
      ),
      builder: (context, sizeClass, child) => WindowSizeContext(
        windowSizeClass: _sizeClass.value,
        child: BackdropFilter(
          filter:
              (sizeClass == WindowSizeClass.compact &&
                  !widget.blurCompactBackground)
              ? ImageFilter.blur()
              : blurFilter,
          // filter: switch (sizeClass) {
          //   WindowSizeClass.compact => ImageFilter.blur(),
          //   _ => blurFilter,
          // },
          child: switch (_sizeClass.value) {
            WindowSizeClass.compact => Dialog.fullscreen(
              backgroundColor: widget.backgroundColor,
              child: child,
            ),
            WindowSizeClass.medium || WindowSizeClass.expanded => Dialog(
              insetPadding: EdgeInsets.zero,
              clipBehavior: Clip.hardEdge,
              backgroundColor: widget.backgroundColor,
              child: child,
            ),
          },
        ),
      ),
    );
  }
}
