import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/global.dart';

void showMessageSnackbar(
  BuildContext context,
  String message, {
  Function()? action,
  String? actionText,
}) {
  final overlay = rootNavKey.currentState?.overlay;
  if (overlay == null) return;

  late OverlayEntry entry;
  entry = OverlayEntry(
    builder: (context) {
      final padding = MediaQuery.of(context).padding;
      return Positioned(
        bottom:
            padding.top +
            64 /* avoid bottom bars */ +
            16 /* part of the margin */,
        left: 16,
        right: 16,
        child: _OverlaySnack(
          message: message,
          duration: Duration(seconds: 4),
          dismissDirection: DismissDirection.vertical,
          onDismiss: () => entry.remove(),
          action: action,
          actionLabel: actionText,
        ),
      );
    },
  );

  overlay.insert(entry);
}

void showErrorSnackbar(BuildContext context, String errorMessage) {
  showMessageSnackbar(
    context,
    errorMessage,
    action: () {
      Clipboard.setData(ClipboardData(text: errorMessage));
      rootScaffoldMessengerKey.currentState?.hideCurrentSnackBar(
        reason: SnackBarClosedReason.action,
      );
      showMessageSnackbar(context, 'Copied to clipboard');
    },
    actionText: 'Copy',
  );
}

/// Swipe-dismissable overlay "snack" (toast-like)
class _OverlaySnack extends StatefulWidget {
  const _OverlaySnack({
    Key? key,
    required this.message,
    required this.onDismiss,
    this.action,
    this.actionLabel,
    this.duration = const Duration(seconds: 3),
    this.dismissDirection = DismissDirection.up,
  }) : super(key: key);

  final String message;
  final Duration duration;
  final DismissDirection dismissDirection;
  final VoidCallback onDismiss;
  final Function()? action;
  final String? actionLabel;

  @override
  State<_OverlaySnack> createState() => _OverlaySnackState();
}

class _OverlaySnackState extends State<_OverlaySnack>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ac = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 160),
  );
  late final Animation<double> _fade = CurvedAnimation(
    parent: _ac,
    curve: Curves.easeOut,
  );

  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _ac.forward();
    _timer = Timer(widget.duration, _animateOut);
  }

  void _animateOut() async {
    // Prevent double removal
    if (!mounted) return;
    await _ac.reverse();
    widget.onDismiss();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _ac.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final action = widget.action;
    final actionLabel = widget.actionLabel;

    return Dismissible(
      key: UniqueKey(),
      direction: widget.dismissDirection,
      onDismissed: (_) {
        _timer?.cancel();
        widget.onDismiss(); // removes the OverlayEntry
      },
      child: FadeTransition(
        opacity: _fade,
        child: Material(
          elevation: 8,
          borderRadius: BorderRadius.circular(4),
          color: theme.colorScheme.inverseSurface,
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: 48),
            child: Padding(
              padding: const EdgeInsets.only(
                left: 16,
                right: 12,
                top: 12,
                bottom: 12,
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                spacing: 12,
                children: [
                  Expanded(
                    child: Text(
                      widget.message,
                      style: TextStyle(
                        color: theme.colorScheme.onInverseSurface,
                      ),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (action != null && actionLabel != null)
                    TextButton(
                      onPressed: () {
                        _timer?.cancel();
                        widget.onDismiss();
                        action();
                      },
                      child: Text(actionLabel),
                      style: TextButton.styleFrom(
                        foregroundColor: theme.colorScheme.inversePrimary,
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
