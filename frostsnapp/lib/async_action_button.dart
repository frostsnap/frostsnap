import 'package:flutter/material.dart';
import 'package:frostsnap/snackbar.dart';

/// A [FilledButton] wrapper for actions that kick off an async
/// side effect (typically a nostr broadcast). Covers the common
/// idle → pending → success/error lifecycle so every call site
/// doesn't re-roll it:
///
/// - Tapping the button runs the provided `onPressed` future.
/// - While the future is pending the button disables itself and
///   shows a small [CircularProgressIndicator] in place of the
///   icon (or the whole child if no icon was supplied).
/// - If the future throws, [onError] fires (default: an error
///   snackbar) and the button re-enables.
/// - If the future resolves successfully, the caller's own logic
///   (e.g. closing a dialog) is responsible for whatever comes
///   next; the button just re-enables.
///
/// Passing `null` for [onPressed] disables the button without
/// triggering the loading state, matching [FilledButton]'s
/// convention.
class AsyncActionButton extends StatefulWidget {
  const AsyncActionButton({
    super.key,
    required this.onPressed,
    required this.child,
    this.icon,
    this.onError,
    this.style,
  });

  final Future<void> Function()? onPressed;
  final Widget child;
  final IconData? icon;
  final void Function(BuildContext, Object)? onError;

  /// Optional [ButtonStyle] forwarded to the underlying [FilledButton].
  /// Use this for variants like a destructive red "Cancel" button.
  final ButtonStyle? style;

  @override
  State<AsyncActionButton> createState() => _AsyncActionButtonState();
}

class _AsyncActionButtonState extends State<AsyncActionButton> {
  bool _loading = false;

  Future<void> _onTap() async {
    final onPressed = widget.onPressed;
    if (onPressed == null || _loading) return;
    setState(() => _loading = true);
    try {
      await onPressed();
    } catch (e) {
      if (!mounted) return;
      final handler = widget.onError ?? _defaultOnError;
      handler(context, e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _defaultOnError(BuildContext context, Object error) {
    showErrorSnackbar(context, '$error');
  }

  static const _spinnerSize = 18.0;
  Widget _spinner() => const SizedBox(
    width: _spinnerSize,
    height: _spinnerSize,
    child: CircularProgressIndicator(strokeWidth: 2),
  );

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null && !_loading;
    final effectiveOnPressed = enabled ? _onTap : null;

    if (widget.icon != null) {
      return FilledButton.icon(
        onPressed: effectiveOnPressed,
        style: widget.style,
        icon: _loading ? _spinner() : Icon(widget.icon),
        label: widget.child,
      );
    }
    return FilledButton(
      onPressed: effectiveOnPressed,
      style: widget.style,
      child: _loading ? _spinner() : widget.child,
    );
  }
}
