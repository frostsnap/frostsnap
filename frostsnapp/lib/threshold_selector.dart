import 'dart:math';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

const _thumbRadius = 11.0;

class ThresholdSelector extends StatefulWidget {
  final int threshold;
  final int totalDevices;
  final int recommendedThreshold;
  final ValueChanged<int> onChanged;

  const ThresholdSelector({
    super.key,
    required this.threshold,
    required this.totalDevices,
    required this.recommendedThreshold,
    required this.onChanged,
  });

  @override
  State<ThresholdSelector> createState() => _ThresholdSelectorState();
}

class _ThresholdSelectorState extends State<ThresholdSelector>
    with TickerProviderStateMixin {
  // Animation controllers
  late final AnimationController _glowController;
  late final AnimationController _pulseController;
  late final AnimationController _numberController;
  late final AnimationController _idleController;
  late final AnimationController _sparkleController;

  // Ring pulse: which threshold value to animate the ring at
  int _ringPulseTarget = 0;

  @override
  void initState() {
    super.initState();

    _glowController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 4),
    )..repeat();

    _pulseController =
        AnimationController(
          vsync: this,
          duration: const Duration(milliseconds: 350),
        )..addStatusListener((status) {
          if (status == AnimationStatus.completed) {
            _pulseController.reverse();
          }
        });

    _numberController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 200),
    );

    _idleController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1500),
    )..repeat();

    _sparkleController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 600),
    );
  }

  @override
  void didUpdateWidget(ThresholdSelector oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.threshold != widget.threshold) {
      _pulseController.forward(from: 0);
      _numberController.forward(from: 0);
      _ringPulseTarget = widget.threshold;
      _sparkleController.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _glowController.dispose();
    _pulseController.dispose();
    _numberController.dispose();
    _idleController.dispose();
    _sparkleController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final n = widget.totalDevices;
    final t = widget.threshold;

    final isRecommended = t == widget.recommendedThreshold && n > 1;
    final showMultiDevice = n > 1;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Center(
          child: AnimatedBuilder(
            animation: _numberController,
            builder: (context, child) {
              final anim = Curves.easeOutCubic.transform(
                _numberController.value,
              );
              final scale = 1.0 + 0.06 * (1.0 - (2 * anim - 1).abs());
              return Transform.scale(scale: scale, child: child);
            },
            child: _HeroDisplay(threshold: t, totalDevices: n),
          ),
        ),
        const SizedBox(height: 4),
        Center(
          child: Text(
            'Devices required to sign',
            style: theme.textTheme.bodySmall?.copyWith(
              color: colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        const SizedBox(height: 6),
        Center(
          child: AnimatedOpacity(
            opacity: isRecommended ? 1.0 : 0.0,
            duration: const Duration(milliseconds: 200),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
              decoration: BoxDecoration(
                color: colorScheme.primaryContainer,
                borderRadius: BorderRadius.circular(12),
              ),
              child: Text(
                'Recommended',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: colorScheme.onPrimaryContainer,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ),
        ),
        const SizedBox(height: 16),
        if (showMultiDevice)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 32),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final trackWidth = constraints.maxWidth;
                const trackPadding = 24.0;
                return Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _buildTrack(colorScheme, theme, trackWidth, trackPadding),
                    _buildSpectrumLabels(theme, colorScheme, trackPadding),
                  ],
                );
              },
            ),
          ),
        const SizedBox(height: 16),
      ],
    );
  }

  Widget _buildTrack(
    ColorScheme colorScheme,
    ThemeData theme,
    double trackWidth,
    double trackPadding,
  ) {
    final n = widget.totalDevices;

    return SizedBox(
      height: 48,
      width: trackWidth,
      child: Stack(
        children: [
          // Visual layer: custom painted track
          Positioned.fill(
            child: AnimatedBuilder(
              animation: Listenable.merge([
                _glowController,
                _pulseController,
                _idleController,
                _sparkleController,
              ]),
              builder: (context, _) {
                return CustomPaint(
                  painter: _TrackPainter(
                    threshold: widget.threshold,
                    totalDevices: n,
                    trackPadding: trackPadding,
                    glowPhase: _glowController.value,
                    pulseValue: _pulseController.value,
                    idlePhase: _idleController.value,
                    ringPulseProgress: _sparkleController.value,
                    ringPulseTarget: _ringPulseTarget,
                    primaryColor: colorScheme.primary,
                    secondaryColor: colorScheme.secondary,
                    tertiaryColor: colorScheme.tertiary,
                    surfaceColor: colorScheme.surfaceContainerHighest,
                    outlineColor: colorScheme.outlineVariant,
                    brightness: theme.brightness,
                  ),
                );
              },
            ),
          ),
          // Invisible Slider for standard drag/tap physics.
          // Padded so its endpoints align with the painted track nodes.
          Positioned.fill(
            child: Padding(
              padding: EdgeInsets.symmetric(
                horizontal: trackPadding - _thumbRadius,
              ),
              child: SliderTheme(
                data: SliderThemeData(
                  trackHeight: 0,
                  activeTrackColor: Colors.transparent,
                  inactiveTrackColor: Colors.transparent,
                  thumbColor: Colors.transparent,
                  overlayColor: Colors.transparent,
                  thumbShape: const _InvisibleThumbShape(radius: _thumbRadius),
                  overlayShape: const RoundSliderOverlayShape(overlayRadius: 0),
                ),
                child: Slider(
                  value: widget.threshold.toDouble(),
                  min: 1,
                  max: n.toDouble(),
                  divisions: max(n - 1, 1),
                  onChanged: (value) {
                    final intValue = value.toInt();
                    if (intValue != widget.threshold) {
                      widget.onChanged(intValue);
                    }
                  },
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildSpectrumLabels(
    ThemeData theme,
    ColorScheme colorScheme,
    double trackPadding,
  ) {
    final labelStyle = theme.textTheme.labelSmall?.copyWith(
      color: colorScheme.onSurfaceVariant.withValues(alpha: 0.6),
      fontSize: 10,
    );
    return Row(
      children: [
        Padding(
          padding: EdgeInsets.only(left: trackPadding),
          child: Text('Loss tolerance', style: labelStyle),
        ),
        const Spacer(),
        Padding(
          padding: EdgeInsets.only(right: trackPadding),
          child: Text('Theft resistance', style: labelStyle),
        ),
      ],
    );
  }
}

class _InvisibleThumbShape extends SliderComponentShape {
  final double radius;
  const _InvisibleThumbShape({required this.radius});

  @override
  Size getPreferredSize(bool isEnabled, bool isDiscrete) =>
      Size.fromRadius(radius);

  @override
  void paint(
    PaintingContext context,
    Offset center, {
    required Animation<double> activationAnimation,
    required Animation<double> enableAnimation,
    required bool isDiscrete,
    required TextPainter labelPainter,
    required RenderBox parentBox,
    required SliderThemeData sliderTheme,
    required TextDirection textDirection,
    required double value,
    required double textScaleFactor,
    required Size sizeWithOverflow,
  }) {
    // Draw nothing.
  }
}

class _HeroDisplay extends StatelessWidget {
  final int threshold;
  final int totalDevices;

  const _HeroDisplay({required this.threshold, required this.totalDevices});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    return FittedBox(
      fit: BoxFit.scaleDown,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.baseline,
        textBaseline: TextBaseline.alphabetic,
        children: [
          Text(
            '$threshold',
            style: theme.textTheme.displayLarge?.copyWith(
              fontWeight: FontWeight.w300,
              color: colorScheme.primary,
              fontSize: 72,
              height: 1.0,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            'of',
            style: theme.textTheme.headlineMedium?.copyWith(
              color: colorScheme.onSurfaceVariant,
              fontWeight: FontWeight.w400,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            '$totalDevices',
            style: theme.textTheme.displayMedium?.copyWith(
              fontWeight: FontWeight.w300,
              color: colorScheme.onSurfaceVariant,
              height: 1.0,
            ),
          ),
        ],
      ),
    );
  }
}

class _TrackPainter extends CustomPainter {
  final int threshold;
  final int totalDevices;
  final double trackPadding;
  final double glowPhase;
  final double pulseValue;
  final double idlePhase;
  final double ringPulseProgress;
  final int ringPulseTarget;
  final Color primaryColor;
  final Color secondaryColor;
  final Color tertiaryColor;
  final Color surfaceColor;
  final Color outlineColor;
  final Brightness brightness;

  _TrackPainter({
    required this.threshold,
    required this.totalDevices,
    required this.trackPadding,
    required this.glowPhase,
    required this.pulseValue,
    required this.idlePhase,
    required this.ringPulseProgress,
    required this.ringPulseTarget,
    required this.primaryColor,
    required this.secondaryColor,
    required this.tertiaryColor,
    required this.surfaceColor,
    required this.outlineColor,
    required this.brightness,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final n = totalDevices;
    if (n <= 1) return;

    final usableWidth = size.width - trackPadding * 2;
    final spacing = usableWidth / (n - 1);
    final trackY = size.height * 0.5;

    double notchX(int value) => trackPadding + (value - 1) * spacing;
    final thumbX = notchX(threshold);

    // Inactive track
    final inactivePaint = Paint()
      ..color = outlineColor.withValues(alpha: 0.25)
      ..strokeWidth = 3.0
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(
      Offset(notchX(1), trackY),
      Offset(notchX(n), trackY),
      inactivePaint,
    );

    // Active track gradient
    final activeStartX = notchX(1);
    final activeEndX = thumbX.clamp(activeStartX, notchX(n));
    if (activeEndX > activeStartX) {
      final gradientColors = [
        primaryColor,
        secondaryColor,
        tertiaryColor,
        primaryColor,
      ];

      final shader = ui.Gradient.linear(
        Offset(activeStartX - size.width * glowPhase, trackY),
        Offset(activeStartX - size.width * glowPhase + size.width, trackY),
        gradientColors,
        [0.0, 0.33, 0.66, 1.0],
        TileMode.repeated,
      );

      final glowIntensity = threshold.toDouble() / totalDevices;

      // Glow behind active track
      final glowPaint = Paint()
        ..shader = shader
        ..strokeWidth = 4.5 + pulseValue
        ..strokeCap = StrokeCap.round
        ..maskFilter = MaskFilter.blur(
          BlurStyle.normal,
          1.5 + glowIntensity * 1.5 + pulseValue,
        );
      canvas.drawLine(
        Offset(activeStartX, trackY),
        Offset(activeEndX, trackY),
        glowPaint,
      );

      // Crisp active track
      final activeTrackPaint = Paint()
        ..shader = shader
        ..strokeWidth = 4.0
        ..strokeCap = StrokeCap.round;
      canvas.drawLine(
        Offset(activeStartX, trackY),
        Offset(activeEndX, trackY),
        activeTrackPaint,
      );
    }

    // Notches
    for (int i = 1; i <= n; i++) {
      final x = notchX(i);
      final isActive = i <= threshold;

      const dotRadius = 6.0;
      final center = Offset(x, trackY);

      if (isActive) {
        final glowPaint = Paint()
          ..color = primaryColor.withValues(alpha: 0.25)
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, 4.0);
        canvas.drawCircle(center, dotRadius + 2, glowPaint);

        final dotPaint = Paint()
          ..color = primaryColor.withValues(alpha: 0.85)
          ..style = PaintingStyle.fill;
        canvas.drawCircle(center, dotRadius, dotPaint);
      } else {
        final ringPaint = Paint()
          ..color = outlineColor.withValues(alpha: 0.5)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2.0;
        canvas.drawCircle(center, dotRadius, ringPaint);

        final fillPaint = Paint()
          ..color = surfaceColor.withValues(alpha: 0.4)
          ..style = PaintingStyle.fill;
        canvas.drawCircle(center, dotRadius, fillPaint);
      }
    }

    // Thumb
    {
      final center = Offset(thumbX, trackY);
      const baseRadius = _thumbRadius;
      final thumbRadius = baseRadius + pulseValue * 2.0;

      final idlePulse = sin(idlePhase * 2 * pi) * 0.5 + 0.5;

      // Shadow
      final shadowPaint = Paint()
        ..color = Colors.black.withValues(alpha: 0.15)
        ..maskFilter = MaskFilter.blur(BlurStyle.normal, 3.0);
      canvas.drawCircle(
        center + const Offset(0, 1.0),
        thumbRadius,
        shadowPaint,
      );

      canvas.save();
      canvas.translate(center.dx, center.dy);

      // Outer glow
      final glowAlpha = 0.12 + pulseValue * 0.08 + idlePulse * 0.4;
      final glowPaint = Paint()
        ..color = primaryColor.withValues(alpha: glowAlpha)
        ..maskFilter = MaskFilter.blur(
          BlurStyle.normal,
          4.0 + pulseValue * 2.0 + idlePulse * 8.0,
        );
      canvas.drawCircle(Offset.zero, thumbRadius + 2, glowPaint);

      // Filled thumb
      final thumbPaint = Paint()
        ..color = primaryColor
        ..style = PaintingStyle.fill;
      canvas.drawCircle(Offset.zero, thumbRadius, thumbPaint);

      // Inner highlight
      final highlightColor = Color.lerp(
        primaryColor,
        Colors.white,
        brightness == Brightness.dark ? 0.25 : 0.35,
      )!;
      final highlightPaint = Paint()
        ..shader = ui.Gradient.radial(
          Offset(-thumbRadius * 0.25, -thumbRadius * 0.25),
          thumbRadius * 0.8,
          [
            highlightColor.withValues(alpha: 0.4),
            primaryColor.withValues(alpha: 0.0),
          ],
        );
      canvas.drawCircle(Offset.zero, thumbRadius, highlightPaint);

      canvas.restore();
    }

    // Snap animation: glow flash + sonar ring
    if (ringPulseProgress > 0 && ringPulseProgress < 1.0) {
      final center = Offset(notchX(ringPulseTarget), trackY);

      // Glow flash
      final flashT = Curves.easeOutCubic.transform(ringPulseProgress);
      final intensity = 1.0 - flashT;
      final flashPaint = Paint()
        ..color = primaryColor.withValues(
          alpha: (intensity * 0.5).clamp(0.0, 1.0),
        )
        ..maskFilter = MaskFilter.blur(BlurStyle.normal, 8.0 + intensity * 8.0);
      canvas.drawCircle(center, 16.0, flashPaint);

      // Sonar ring
      final sonarT = Curves.easeOut.transform(ringPulseProgress);
      final sonarRadius = 12.0 + sonarT * 18.0;
      final sonarAlpha = (1.0 - sonarT) * 0.2;
      final sonarPaint = Paint()
        ..color = primaryColor.withValues(alpha: sonarAlpha.clamp(0.0, 1.0))
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.5 * (1.0 - sonarT);
      canvas.drawCircle(center, sonarRadius, sonarPaint);
    }
  }

  @override
  bool shouldRepaint(_TrackPainter oldDelegate) => true;
}
