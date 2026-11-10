import 'package:flutter/material.dart';

/// A round button that fires only after being held for [holdDuration].
/// Releasing early resets it. Generalised from `HoldToDeleteButton`
/// (settings.dart) with the colour and the duration as parameters.
class HoldToConfirmButton extends StatefulWidget {
  final VoidCallback? onComplete;
  final Widget label;

  /// The button's fill. Defaults to the primary colour.
  final Color? color;

  /// The label and progress colour. Defaults to the colour on [color].
  final Color? onColor;
  final Duration holdDuration;
  final double size;

  const HoldToConfirmButton({
    super.key,
    required this.onComplete,
    required this.label,
    this.color,
    this.onColor,
    this.holdDuration = const Duration(seconds: 2),
    this.size = 140,
  });

  @override
  State<HoldToConfirmButton> createState() => _HoldToConfirmButtonState();
}

class _HoldToConfirmButtonState extends State<HoldToConfirmButton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _progress;

  @override
  void initState() {
    super.initState();
    _progress = AnimationController(vsync: this, duration: widget.holdDuration)
      ..addStatusListener((status) {
        if (status == AnimationStatus.completed) {
          _progress.value = 0;
          widget.onComplete?.call();
        }
      });
  }

  @override
  void didUpdateWidget(covariant HoldToConfirmButton oldWidget) {
    super.didUpdateWidget(oldWidget);
    _progress.duration = widget.holdDuration;
    if (widget.onComplete == null) _progress.value = 0;
  }

  @override
  void dispose() {
    _progress.dispose();
    super.dispose();
  }

  void _start() {
    if (widget.onComplete == null) return;
    _progress.forward(from: 0);
  }

  void _cancel() {
    if (_progress.status != AnimationStatus.completed) _progress.value = 0;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled = widget.onComplete != null;
    final color = enabled
        ? widget.color ?? theme.colorScheme.primary
        : theme.colorScheme.surfaceContainerHighest;
    final onColor = enabled
        ? widget.onColor ??
              (widget.color == null
                  ? theme.colorScheme.onPrimary
                  : ThemeData.estimateBrightnessForColor(color) ==
                        Brightness.dark
                  ? Colors.white
                  : Colors.black)
        : theme.colorScheme.onSurfaceVariant;

    return Semantics(
      button: true,
      enabled: enabled,
      hint: 'Press and hold',
      // A screen reader's long press is the deliberate act that stands in
      // for the hold.
      onLongPress: widget.onComplete,
      child: GestureDetector(
        onTapDown: (_) => _start(),
        onTapUp: (_) => _cancel(),
        onTapCancel: _cancel,
        child: AnimatedBuilder(
          animation: _progress,
          builder: (context, label) {
            final pressed = _progress.isAnimating;
            return AnimatedContainer(
              duration: Durations.short2,
              width: widget.size,
              height: widget.size,
              decoration: BoxDecoration(
                color: ElevationOverlay.applySurfaceTint(
                  color,
                  Colors.black,
                  pressed ? 10.0 : 0.0,
                ),
                shape: BoxShape.circle,
                boxShadow: pressed || !enabled
                    ? const []
                    : const [
                        BoxShadow(
                          color: Colors.black26,
                          offset: Offset(0, 4),
                          blurRadius: 4.0,
                        ),
                      ],
              ),
              child: Stack(
                alignment: Alignment.center,
                children: [
                  SizedBox.square(
                    dimension: widget.size,
                    child: CircularProgressIndicator(
                      value: _progress.value,
                      strokeWidth: 5,
                      backgroundColor: Colors.transparent,
                      valueColor: AlwaysStoppedAnimation(onColor),
                    ),
                  ),
                  Padding(
                    padding: const EdgeInsets.all(16),
                    child: DefaultTextStyle(
                      style: theme.textTheme.labelLarge!.copyWith(
                        color: onColor,
                      ),
                      textAlign: TextAlign.center,
                      child: label!,
                    ),
                  ),
                ],
              ),
            );
          },
          child: widget.label,
        ),
      ),
    );
  }
}
