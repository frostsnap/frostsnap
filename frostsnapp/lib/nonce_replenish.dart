import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';

/// Minimal nonce replenishment widget for use in wallet creation flow.
/// Shows aggregate progress with smooth time-based animation.
class MinimalNonceReplenishWidget extends StatefulWidget {
  final Stream<NonceReplenishState> stream;
  final VoidCallback? onComplete;
  final VoidCallback? onAbort;
  final bool autoAdvance;

  const MinimalNonceReplenishWidget({
    super.key,
    required this.stream,
    this.onComplete,
    this.onAbort,
    this.autoAdvance = false,
  });

  @override
  State<MinimalNonceReplenishWidget> createState() =>
      _MinimalNonceReplenishWidgetState();
}

class _MinimalNonceReplenishWidgetState
    extends State<MinimalNonceReplenishWidget>
    with SingleTickerProviderStateMixin {
  late final AnimationController _animController;
  StreamSubscription<NonceReplenishState>? _sub;
  NonceReplenishState? _state;

  double _displayed = 0;

  // We animate a line segment from (_anchorValue, _anchorTime) toward 0.98
  // over _segmentDurationSecs. When a real progress update arrives, we
  // re-anchor at the current display position and recompute how long the
  // remaining animation should take. This means:
  //   - the curve always starts from where we currently are (no jumps)
  //   - it always moves forward (lerp toward 0.98 is monotonic)
  //   - estimate changes just adjust the speed, not the position
  DateTime _anchorTime = DateTime.now();
  double _anchorValue = 0;
  double _segmentDurationSecs = 60.0;

  DateTime? _processStart;
  bool _hasRealEstimate = false;

  bool _completeFired = false;
  bool _abortFired = false;

  // Completion fill: time-based ease from _completionStartValue to 1.0
  DateTime? _completionStartTime;
  double _completionStartValue = 0;
  static const _completionFillDuration = Duration(milliseconds: 600);

  // 0..1 phase for completion celebration (flash + scale bounce).
  // Runs over ~800ms after _displayed hits 1.0.
  DateTime? _celebrationStartTime;
  double _completionPhase = 0.0;
  static const _celebrationDuration = Duration(milliseconds: 800);

  @override
  void initState() {
    super.initState();
    _animController = AnimationController.unbounded(vsync: this)
      ..addListener(_onTick)
      ..repeat(min: 0, max: 1, period: const Duration(seconds: 4));
    _subscribe();
  }

  @override
  void didUpdateWidget(covariant MinimalNonceReplenishWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.stream != widget.stream) {
      _sub?.cancel();
      _subscribe();
    }
  }

  void _subscribe() {
    _sub = widget.stream.listen(_onStateUpdate, onError: (_) {});
  }

  void _onStateUpdate(NonceReplenishState state) {
    if (!mounted) return;

    _processStart ??= DateTime.now();

    // Re-anchor from current display position and estimate remaining time.
    if (state.totalStreams > 0 &&
        state.completedStreams > 0 &&
        !state.isFinished()) {
      final real = state.completedStreams / state.totalStreams;
      final totalElapsed =
          DateTime.now().difference(_processStart!).inMilliseconds / 1000.0;

      if (totalElapsed > 0.1) {
        _hasRealEstimate = true;

        final estimatedRemaining = ((totalElapsed / real) - totalElapsed).clamp(
          0.5,
          120.0,
        );

        _anchorValue = _displayed;
        _anchorTime = DateTime.now();
        _segmentDurationSecs = estimatedRemaining;
      }
    }

    setState(() {
      _state = state;
    });

    if (state.abort && !_abortFired && widget.onAbort != null) {
      _abortFired = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        widget.onAbort!();
      });
    }

    // onComplete is fired from _onTick after the animation reaches 100%,
    // not here, so the user sees the full circle before transitioning.
  }

  void _onTick() {
    if (_state == null || !mounted) return;

    if (_state!.isFinished()) {
      if (_displayed < 1.0) {
        if (_completionStartTime == null) {
          _completionStartTime = DateTime.now();
          _completionStartValue = _displayed;
        }

        final elapsed = DateTime.now()
            .difference(_completionStartTime!)
            .inMilliseconds;
        final t = (elapsed / _completionFillDuration.inMilliseconds).clamp(
          0.0,
          1.0,
        );
        final eased = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
        _displayed =
            _completionStartValue + (1.0 - _completionStartValue) * eased;

        if (t >= 1.0) _displayed = 1.0;
        setState(() {});
      } else {
        _celebrationStartTime ??= DateTime.now();
        final celebElapsed = DateTime.now()
            .difference(_celebrationStartTime!)
            .inMilliseconds;
        _completionPhase = (celebElapsed / _celebrationDuration.inMilliseconds)
            .clamp(0.0, 1.0);
        setState(() {});

        if (_completionPhase >= 1.0) {
          _animController.stop();
        }

        if (!_completeFired &&
            widget.autoAdvance &&
            widget.onComplete != null &&
            _completionPhase >= 0.6) {
          _completeFired = true;
          Future.delayed(const Duration(milliseconds: 300), () {
            if (mounted) widget.onComplete!();
          });
        }
      }
      return;
    }

    if (!_hasRealEstimate) return;

    final elapsed =
        DateTime.now().difference(_anchorTime).inMilliseconds / 1000.0;
    final t = elapsed / _segmentDurationSecs;

    // Approaches 1.0 asymptotically, never stalls. Re-anchoring on each
    // real progress update resets the curve so it stays responsive.
    final curve = 1.0 - 1.0 / (1.0 + t);

    const ceiling = 0.98;
    final target = _anchorValue + (ceiling - _anchorValue) * curve;

    if (target > _displayed) {
      _displayed = target;
      setState(() {});
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    _animController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isComplete = _displayed >= 1.0;
    final displayPercent = (_displayed * 100).toInt().clamp(0, 100);

    final completionScale = _completionPhase > 0.0
        ? 1.0 + 0.03 * math.sin(_completionPhase.clamp(0.0, 1.0) * math.pi)
        : 1.0;

    final Widget centerLabel;
    if (isComplete) {
      centerLabel = Icon(
        Icons.check_rounded,
        key: const ValueKey('check'),
        size: 48,
        color: theme.colorScheme.primary,
      );
    } else if (_state == null) {
      centerLabel = SizedBox.shrink(key: const ValueKey('empty'));
    } else {
      centerLabel = Text(
        '$displayPercent%',
        key: const ValueKey('percent'),
        style: theme.textTheme.headlineMedium?.copyWith(
          fontWeight: FontWeight.w400,
          color: theme.colorScheme.onSurface,
        ),
      );
    }

    final String subtitle;
    if (isComplete) {
      subtitle = 'Ready';
    } else if (_state == null) {
      subtitle = 'Connecting...';
    } else {
      subtitle = 'Please wait...';
    }

    const double ringSize = 176;

    return Padding(
      padding: EdgeInsets.symmetric(vertical: 32),
      child: Align(
        alignment: Alignment.topCenter,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Preparing devices', style: theme.textTheme.headlineMedium),
            SizedBox(height: 40),
            SizedBox(
              width: ringSize,
              height: ringSize,
              child: Transform.scale(
                scale: completionScale,
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    CustomPaint(
                      size: const Size(ringSize, ringSize),
                      painter: _GlowingProgressPainter(
                        fraction: _displayed,
                        trackColor: theme.colorScheme.surfaceContainerHighest,
                        primaryColor: theme.colorScheme.primary,
                        secondaryColor: theme.colorScheme.secondary,
                        tertiaryColor: theme.colorScheme.tertiary,
                        strokeWidth: 6,
                        pulsePhase: _animController.value,
                        completionPhase: _completionPhase,
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
            ),
            SizedBox(height: 24),
            AnimatedSwitcher(
              duration: const Duration(milliseconds: 300),
              child: Text(
                subtitle,
                key: ValueKey(subtitle),
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: isComplete
                      ? theme.colorScheme.primary
                      : theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _GlowingProgressPainter extends CustomPainter {
  final double fraction;
  final Color trackColor;
  final Color primaryColor;
  final Color secondaryColor;
  final Color tertiaryColor;
  final double strokeWidth;
  final double pulsePhase;
  final double completionPhase;
  final Brightness brightness;

  _GlowingProgressPainter({
    required this.fraction,
    required this.trackColor,
    required this.primaryColor,
    required this.secondaryColor,
    required this.tertiaryColor,
    required this.strokeWidth,
    required this.pulsePhase,
    this.completionPhase = 0.0,
    required this.brightness,
  });

  ui.Shader _makeGradientShader(Rect rect, double alpha) {
    final gradientColors = [
      primaryColor.withValues(alpha: alpha),
      secondaryColor.withValues(alpha: alpha),
      tertiaryColor.withValues(alpha: alpha),
      primaryColor.withValues(alpha: alpha),
    ];
    final shift = rect.width * pulsePhase;
    return ui.Gradient.linear(
      Offset(rect.left - shift, rect.center.dy),
      Offset(rect.left - shift + rect.width, rect.center.dy),
      gradientColors,
      [0.0, 0.33, 0.66, 1.0],
      TileMode.repeated,
    );
  }

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final radius = (math.min(size.width, size.height) - strokeWidth) / 2 - 8;
    final rect = Rect.fromCircle(center: center, radius: radius);
    const startAngle = -math.pi / 2;
    final sweepAngle = 2 * math.pi * fraction.clamp(0.0, 1.0);

    final rawPulse = math.sin(pulsePhase * 2 * math.pi);
    final smoothPulse = rawPulse.abs() * rawPulse;
    final pulseStrength = fraction >= 1.0
        ? 0.0
        : 0.07 * (1.0 - completionPhase);
    final pulse = 0.93 + pulseStrength * smoothPulse;

    // Brief intensity spike that peaks around completionPhase=0.3 and fades.
    final flashIntensity = completionPhase > 0.0
        ? (math.sin(completionPhase.clamp(0.0, 0.6) / 0.6 * math.pi) * 0.4)
        : 0.0;

    // Track
    final trackPaint = Paint()
      ..color = trackColor.withValues(alpha: 0.6)
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round;
    canvas.drawCircle(center, radius, trackPaint);

    if (fraction <= 0) return;

    final glowBoost = 1.0 + flashIntensity;

    // Outer glow (diffuse, wide)
    final outerGlowAlpha = (0.10 * pulse * glowBoost).clamp(0.0, 1.0);
    final outerGlowPaint = Paint()
      ..shader = _makeGradientShader(rect, outerGlowAlpha)
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth + 16
      ..strokeCap = StrokeCap.butt
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 10);
    canvas.drawArc(rect, startAngle, sweepAngle, false, outerGlowPaint);

    // Inner glow (tighter, brighter)
    final innerGlowAlpha = (0.22 * pulse * glowBoost).clamp(0.0, 1.0);
    final innerGlowPaint = Paint()
      ..shader = _makeGradientShader(rect, innerGlowAlpha)
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth + 5
      ..strokeCap = StrokeCap.butt
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4);
    canvas.drawArc(rect, startAngle, sweepAngle, false, innerGlowPaint);

    // Progress arc
    final bodyAlpha = (pulse + flashIntensity * 0.3).clamp(0.0, 1.0);
    final arcPaint = Paint()
      ..shader = _makeGradientShader(rect, bodyAlpha)
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round;
    canvas.drawArc(rect, startAngle, sweepAngle, false, arcPaint);

    // Leading-edge glow (fades out on completion)
    final tipAngle = startAngle + sweepAngle;
    final tipCenter = Offset(
      center.dx + radius * math.cos(tipAngle),
      center.dy + radius * math.sin(tipAngle),
    );
    final tipFade = 1.0 - completionPhase;
    if (tipFade > 0.01) {
      final whiteBlend = brightness == Brightness.dark ? 0.25 : 0.35;
      final brightColor = Color.lerp(primaryColor, Colors.white, whiteBlend)!;

      final bloomRadius = (strokeWidth * 2.0 + 6) * pulse;
      final bloomPaint = Paint()
        ..shader = ui.Gradient.radial(
          tipCenter,
          bloomRadius,
          [
            primaryColor.withValues(
              alpha: (0.40 * pulse * tipFade).clamp(0.0, 1.0),
            ),
            primaryColor.withValues(alpha: 0.0),
          ],
          [0.0, 1.0],
        );
      canvas.drawCircle(tipCenter, bloomRadius, bloomPaint);

      final hotspotRadius = strokeWidth * 0.65;
      final hotspotPaint = Paint()
        ..shader = ui.Gradient.radial(
          tipCenter,
          hotspotRadius,
          [
            brightColor.withValues(
              alpha: (0.90 * pulse * tipFade).clamp(0.0, 1.0),
            ),
            primaryColor.withValues(alpha: 0.0),
          ],
          [0.0, 1.0],
        );
      canvas.drawCircle(tipCenter, hotspotRadius, hotspotPaint);
    }

    // Completion flash: full-ring bloom that fades
    if (flashIntensity > 0.01) {
      final flashPaint = Paint()
        ..shader = _makeGradientShader(
          rect,
          (flashIntensity * 0.5).clamp(0.0, 1.0),
        )
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth + 20
        ..strokeCap = StrokeCap.round
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 14);
      canvas.drawCircle(center, radius, flashPaint);
    }
  }

  @override
  bool shouldRepaint(_GlowingProgressPainter old) =>
      old.fraction != fraction ||
      old.pulsePhase != pulsePhase ||
      old.completionPhase != completionPhase ||
      old.primaryColor != primaryColor ||
      old.secondaryColor != secondaryColor ||
      old.tertiaryColor != tertiaryColor;
}

/// Full-screen nonce replenishment dialog with Done/Cancel buttons.
/// Used after wallet restoration where user interaction is required.
class NonceReplenishDialog extends StatelessWidget {
  final Stream<NonceReplenishState> stream;
  final VoidCallback? onCancel;

  const NonceReplenishDialog({super.key, required this.stream, this.onCancel});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mediaQuery = MediaQuery.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: CustomScrollView(
            physics: ClampingScrollPhysics(),
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
                padding: const EdgeInsets.fromLTRB(20, 20, 20, 28),
                sliver: SliverToBoxAdapter(
                  child: MinimalNonceReplenishWidget(
                    stream: stream,
                    autoAdvance: false,
                  ),
                ),
              ),
              SliverPadding(padding: EdgeInsets.only(bottom: 32)),
            ],
          ),
        ),
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Divider(height: 0),
            Padding(
              padding: EdgeInsets.all(
                20,
              ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
              child: SafeArea(
                top: false,
                child: StreamBuilder<NonceReplenishState>(
                  stream: stream,
                  builder: (context, snapshot) {
                    final state = snapshot.data;
                    final allComplete = state?.isFinished() ?? false;
                    final isAborted = state?.abort ?? false;

                    return Row(
                      mainAxisSize: MainAxisSize.max,
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Flexible(
                          child: TextButton(
                            onPressed: (!allComplete && !isAborted)
                                ? onCancel
                                : null,
                            child: Text(
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
                              onPressed: allComplete
                                  ? () => Navigator.pop(context, true)
                                  : null,
                              child: Text(
                                allComplete ? 'Done' : 'Please wait...',
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
