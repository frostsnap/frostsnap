import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:rxdart/rxdart.dart';

sealed class NonceReplenishTerminal {
  const NonceReplenishTerminal();
}

final class NonceReplenishCompleted extends NonceReplenishTerminal {
  const NonceReplenishCompleted();
}

final class NonceReplenishAborted extends NonceReplenishTerminal {
  const NonceReplenishAborted();
}

final class NonceReplenishFailed extends NonceReplenishTerminal {
  final Object error;

  const NonceReplenishFailed(this.error);
}

class SmoothProgressCircle extends StatefulWidget {
  final double progress;
  final bool complete;
  final double size;
  final double strokeWidth;
  final VoidCallback? onCelebrationComplete;

  const SmoothProgressCircle({
    super.key,
    required this.progress,
    required this.complete,
    this.size = 176,
    this.strokeWidth = 6,
    this.onCelebrationComplete,
  });

  @override
  State<SmoothProgressCircle> createState() => _SmoothProgressCircleState();
}

class _SmoothProgressCircleState extends State<SmoothProgressCircle>
    with TickerProviderStateMixin {
  static const _valueEaseDuration = Duration(milliseconds: 800);
  static const _completionFillDuration = Duration(milliseconds: 600);
  static const _celebrationDuration = Duration(milliseconds: 800);
  static const _pulsePeriod = Duration(seconds: 4);

  late final AnimationController _pulse;
  late final AnimationController _celebration;
  late final AnimationController _value;
  bool _completing = false;

  @override
  void initState() {
    super.initState();
    _pulse = AnimationController(vsync: this, duration: _pulsePeriod)..repeat();
    _celebration = AnimationController(
      vsync: this,
      duration: _celebrationDuration,
    );
    _value = AnimationController(vsync: this)
      ..value = widget.progress.clamp(0.0, 1.0);
    if (widget.complete) _playCompletion();
  }

  @override
  void didUpdateWidget(covariant SmoothProgressCircle old) {
    super.didUpdateWidget(old);
    final clamped = widget.progress.clamp(0.0, 1.0);
    if (!widget.complete && old.complete) {
      _completing = false;
      _celebration
        ..stop()
        ..value = 0.0;
      _value
        ..stop()
        ..value = clamped;
      return;
    }
    if (!_completing && widget.progress != old.progress) {
      _value.animateTo(
        clamped,
        duration: _valueEaseDuration,
        curve: Curves.easeOut,
      );
    }
    if (widget.complete && !old.complete) _playCompletion();
  }

  Future<void> _playCompletion() async {
    _completing = true;
    try {
      await _value.animateTo(
        1.0,
        duration: _completionFillDuration,
        curve: Curves.easeOutCubic,
      );
      if (!mounted) return;
      await _celebration.forward(from: 0.0);
      if (!mounted) return;
      widget.onCelebrationComplete?.call();
    } on TickerCanceled {
      // widget disposed mid-animation
    }
  }

  @override
  void dispose() {
    _pulse.dispose();
    _celebration.dispose();
    _value.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return AnimatedBuilder(
      animation: Listenable.merge([_pulse, _celebration, _value]),
      builder: (context, _) {
        final displayed = _value.value;
        final celebration = _celebration.value;
        final isComplete = displayed >= 1.0;
        final bounce = celebration > 0
            ? 1.0 + 0.03 * math.sin(celebration * math.pi)
            : 1.0;

        final Widget centerLabel = isComplete
            ? Icon(
                Icons.check_rounded,
                key: const ValueKey('check'),
                size: 48,
                color: theme.colorScheme.primary,
              )
            : Text(
                '${(displayed * 100).toInt().clamp(0, 100)}%',
                key: const ValueKey('percent'),
                style: theme.textTheme.headlineMedium?.copyWith(
                  fontWeight: FontWeight.w400,
                  color: theme.colorScheme.onSurface,
                ),
              );

        return SizedBox(
          width: widget.size,
          height: widget.size,
          child: Transform.scale(
            scale: bounce,
            child: Stack(
              alignment: Alignment.center,
              children: [
                CustomPaint(
                  size: Size.square(widget.size),
                  painter: _GlowingProgressPainter(
                    fraction: displayed,
                    trackColor: theme.colorScheme.surfaceContainerHighest,
                    primaryColor: theme.colorScheme.primary,
                    secondaryColor: theme.colorScheme.secondary,
                    tertiaryColor: theme.colorScheme.tertiary,
                    strokeWidth: widget.strokeWidth,
                    pulsePhase: _pulse.value,
                    celebrationPhase: celebration,
                    brightness: theme.brightness,
                  ),
                ),
                AnimatedSwitcher(
                  duration: const Duration(milliseconds: 400),
                  switchInCurve: Curves.easeOutBack,
                  switchOutCurve: Curves.easeIn,
                  child: centerLabel,
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _GlowingProgressPainter extends CustomPainter {
  static const _ringInset = 8.0;
  static const _outerGlowWidthBoost = 16.0;
  static const _outerGlowBlur = 10.0;
  static const _outerGlowBaseAlpha = 0.10;
  static const _innerGlowWidthBoost = 5.0;
  static const _innerGlowBlur = 4.0;
  static const _innerGlowBaseAlpha = 0.22;
  static const _flashWidthBoost = 20.0;
  static const _flashBlur = 14.0;
  static const _tipBloomWidthMultiplier = 2.0;
  static const _tipBloomWidthBonus = 6.0;
  static const _tipHotspotWidthMultiplier = 0.65;
  static const _pulseFloor = 0.93;
  static const _pulseRange = 0.07;

  final double fraction;
  final Color trackColor;
  final Color primaryColor;
  final Color secondaryColor;
  final Color tertiaryColor;
  final double strokeWidth;
  final double pulsePhase;
  final double celebrationPhase;
  final Brightness brightness;

  _GlowingProgressPainter({
    required this.fraction,
    required this.trackColor,
    required this.primaryColor,
    required this.secondaryColor,
    required this.tertiaryColor,
    required this.strokeWidth,
    required this.pulsePhase,
    required this.celebrationPhase,
    required this.brightness,
  });

  ui.Shader _bandShader(Rect rect, double alpha) {
    final shift = rect.width * pulsePhase;
    return ui.Gradient.linear(
      Offset(rect.left - shift, rect.center.dy),
      Offset(rect.left - shift + rect.width, rect.center.dy),
      [
        primaryColor.withValues(alpha: alpha),
        secondaryColor.withValues(alpha: alpha),
        tertiaryColor.withValues(alpha: alpha),
        primaryColor.withValues(alpha: alpha),
      ],
      const [0.0, 0.33, 0.66, 1.0],
      TileMode.repeated,
    );
  }

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final radius =
        (math.min(size.width, size.height) - strokeWidth) / 2 - _ringInset;
    final rect = Rect.fromCircle(center: center, radius: radius);
    const startAngle = -math.pi / 2;
    final sweepAngle = 2 * math.pi * fraction.clamp(0.0, 1.0);

    final rawPulse = math.sin(pulsePhase * 2 * math.pi);
    final pulseShape = rawPulse.abs() * rawPulse;
    final pulseStrength = fraction >= 1.0
        ? 0.0
        : _pulseRange * (1.0 - celebrationPhase);
    final pulse = _pulseFloor + pulseStrength * pulseShape;

    final flashIntensity = celebrationPhase > 0.0
        ? math.sin(celebrationPhase.clamp(0.0, 0.6) / 0.6 * math.pi) * 0.4
        : 0.0;
    final glowBoost = 1.0 + flashIntensity;

    final trackPaint = Paint()
      ..color = trackColor.withValues(alpha: 0.6)
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round;
    canvas.drawCircle(center, radius, trackPaint);

    if (fraction <= 0) return;

    final outerAlpha = (_outerGlowBaseAlpha * pulse * glowBoost).clamp(
      0.0,
      1.0,
    );
    canvas.drawArc(
      rect,
      startAngle,
      sweepAngle,
      false,
      Paint()
        ..shader = _bandShader(rect, outerAlpha)
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth + _outerGlowWidthBoost
        ..strokeCap = StrokeCap.butt
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, _outerGlowBlur),
    );

    final innerAlpha = (_innerGlowBaseAlpha * pulse * glowBoost).clamp(
      0.0,
      1.0,
    );
    canvas.drawArc(
      rect,
      startAngle,
      sweepAngle,
      false,
      Paint()
        ..shader = _bandShader(rect, innerAlpha)
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth + _innerGlowWidthBoost
        ..strokeCap = StrokeCap.butt
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, _innerGlowBlur),
    );

    final bodyAlpha = (pulse + flashIntensity * 0.3).clamp(0.0, 1.0);
    canvas.drawArc(
      rect,
      startAngle,
      sweepAngle,
      false,
      Paint()
        ..shader = _bandShader(rect, bodyAlpha)
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth
        ..strokeCap = StrokeCap.round,
    );

    final tipFade = 1.0 - celebrationPhase;
    if (tipFade > 0.01) {
      final tipAngle = startAngle + sweepAngle;
      final tipCenter = Offset(
        center.dx + radius * math.cos(tipAngle),
        center.dy + radius * math.sin(tipAngle),
      );
      final whiteBlend = brightness == Brightness.dark ? 0.25 : 0.35;
      final brightColor = Color.lerp(primaryColor, Colors.white, whiteBlend)!;

      final bloomRadius =
          (strokeWidth * _tipBloomWidthMultiplier + _tipBloomWidthBonus) *
          pulse;
      canvas.drawCircle(
        tipCenter,
        bloomRadius,
        Paint()
          ..shader = ui.Gradient.radial(
            tipCenter,
            bloomRadius,
            [
              primaryColor.withValues(
                alpha: (0.40 * pulse * tipFade).clamp(0.0, 1.0),
              ),
              primaryColor.withValues(alpha: 0.0),
            ],
            const [0.0, 1.0],
          ),
      );

      final hotspotRadius = strokeWidth * _tipHotspotWidthMultiplier;
      canvas.drawCircle(
        tipCenter,
        hotspotRadius,
        Paint()
          ..shader = ui.Gradient.radial(
            tipCenter,
            hotspotRadius,
            [
              brightColor.withValues(
                alpha: (0.90 * pulse * tipFade).clamp(0.0, 1.0),
              ),
              primaryColor.withValues(alpha: 0.0),
            ],
            const [0.0, 1.0],
          ),
      );
    }

    if (flashIntensity > 0.01) {
      canvas.drawCircle(
        center,
        radius,
        Paint()
          ..shader = _bandShader(rect, (flashIntensity * 0.5).clamp(0.0, 1.0))
          ..style = PaintingStyle.stroke
          ..strokeWidth = strokeWidth + _flashWidthBoost
          ..strokeCap = StrokeCap.round
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, _flashBlur),
      );
    }
  }

  @override
  bool shouldRepaint(_GlowingProgressPainter old) =>
      old.fraction != fraction ||
      old.trackColor != trackColor ||
      old.strokeWidth != strokeWidth ||
      old.pulsePhase != pulsePhase ||
      old.celebrationPhase != celebrationPhase ||
      old.primaryColor != primaryColor ||
      old.secondaryColor != secondaryColor ||
      old.tertiaryColor != tertiaryColor ||
      old.brightness != brightness;
}

class NonceReplenishIndicator extends StatefulWidget {
  final ValueStream<NonceReplenishState> stream;
  final ValueChanged<NonceReplenishTerminal>? onTerminal;

  const NonceReplenishIndicator({
    super.key,
    required this.stream,
    this.onTerminal,
  });

  @override
  State<NonceReplenishIndicator> createState() =>
      _NonceReplenishIndicatorState();
}

class _NonceReplenishIndicatorState extends State<NonceReplenishIndicator> {
  StreamSubscription<NonceReplenishState>? _sub;
  NonceReplenishState? _state;
  NonceReplenishTerminal? _terminal;

  @override
  void initState() {
    super.initState();
    _state = widget.stream.valueOrNull;
    _subscribe();
  }

  @override
  void didUpdateWidget(covariant NonceReplenishIndicator old) {
    super.didUpdateWidget(old);
    if (old.stream != widget.stream) {
      _sub?.cancel();
      _state = widget.stream.valueOrNull;
      _terminal = null;
      _subscribe();
    }
  }

  void _emitTerminal(NonceReplenishTerminal terminal) {
    if (_terminal != null) return;
    setState(() => _terminal = terminal);
    widget.onTerminal?.call(terminal);
  }

  void _subscribe() {
    _sub = widget.stream.listen(
      (state) {
        if (!mounted) return;
        setState(() => _state = state);
        if (state.abort) _emitTerminal(const NonceReplenishAborted());
      },
      onError: (Object e) {
        if (!mounted) return;
        _emitTerminal(NonceReplenishFailed(e));
      },
    );
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  double get _progress {
    final s = _state;
    if (s == null || s.totalStreams == 0) return 0;
    return (s.completedStreams / s.totalStreams).clamp(0.0, 1.0);
  }

  bool get _isComplete => _state?.isFinished() ?? false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final complete = _isComplete;
    final terminal = _terminal;

    final (String subtitle, Color subtitleColor) = switch (terminal) {
      NonceReplenishCompleted() => ('Ready', theme.colorScheme.primary),
      NonceReplenishAborted() => ('Disconnected', theme.colorScheme.error),
      NonceReplenishFailed() => ('Failed', theme.colorScheme.error),
      null when _state == null => (
        'Connecting...',
        theme.colorScheme.onSurfaceVariant,
      ),
      _ => ('Please wait...', theme.colorScheme.onSurfaceVariant),
    };

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        SmoothProgressCircle(
          progress: _progress,
          complete: complete,
          onCelebrationComplete: () {
            _emitTerminal(const NonceReplenishCompleted());
          },
        ),
        const SizedBox(height: 24),
        AnimatedSwitcher(
          duration: const Duration(milliseconds: 300),
          child: Text(
            subtitle,
            key: ValueKey(subtitle),
            style: theme.textTheme.bodyMedium?.copyWith(color: subtitleColor),
          ),
        ),
      ],
    );
  }
}

class NonceReplenishDialog extends StatefulWidget {
  final ValueStream<NonceReplenishState> stream;
  final VoidCallback? onCancel;

  const NonceReplenishDialog({super.key, required this.stream, this.onCancel});

  @override
  State<NonceReplenishDialog> createState() => _NonceReplenishDialogState();
}

class _NonceReplenishDialogState extends State<NonceReplenishDialog> {
  NonceReplenishTerminal? _terminal;

  void _handleTerminal(NonceReplenishTerminal terminal) {
    if (!mounted || _terminal != null) return;
    setState(() => _terminal = terminal);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mediaQuery = MediaQuery.of(context);
    final terminal = _terminal;
    final canClose = terminal != null;
    final closeLabel = switch (terminal) {
      NonceReplenishCompleted() => 'Done',
      NonceReplenishAborted() || NonceReplenishFailed() => 'Close',
      null => 'Please wait...',
    };
    final closeValue = terminal is NonceReplenishCompleted;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: CustomScrollView(
            physics: const ClampingScrollPhysics(),
            shrinkWrap: true,
            slivers: [
              SliverAppBar(
                title: Text(
                  'Preparing devices',
                  style: theme.textTheme.titleMedium,
                ),
                automaticallyImplyLeading: false,
                pinned: true,
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(20, 32, 20, 28),
                sliver: SliverToBoxAdapter(
                  child: Align(
                    alignment: Alignment.topCenter,
                    child: NonceReplenishIndicator(
                      stream: widget.stream,
                      onTerminal: _handleTerminal,
                    ),
                  ),
                ),
              ),
              const SliverPadding(padding: EdgeInsets.only(bottom: 32)),
            ],
          ),
        ),
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Divider(height: 0),
            Padding(
              padding: const EdgeInsets.all(
                20,
              ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
              child: SafeArea(
                top: false,
                child: StreamBuilder<NonceReplenishState>(
                  stream: widget.stream,
                  initialData: widget.stream.valueOrNull,
                  builder: (context, snapshot) {
                    final state = snapshot.data;
                    final canCancel =
                        terminal == null && !(state?.abort ?? false);

                    return Row(
                      mainAxisSize: MainAxisSize.max,
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Flexible(
                          child: TextButton(
                            onPressed: canCancel ? widget.onCancel : null,
                            child: const Text(
                              'Cancel',
                              softWrap: false,
                              overflow: TextOverflow.fade,
                            ),
                          ),
                        ),
                        Expanded(
                          flex: 2,
                          child: Align(
                            alignment: AlignmentDirectional.centerEnd,
                            child: FilledButton(
                              onPressed: canClose
                                  ? () => Navigator.pop(context, closeValue)
                                  : null,
                              child: Text(
                                closeLabel,
                                softWrap: false,
                                overflow: TextOverflow.fade,
                              ),
                            ),
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
