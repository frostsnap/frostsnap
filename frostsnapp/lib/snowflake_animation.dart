import 'package:flutter/material.dart';
import 'dart:math' as math;

Offset midpoint(Offset p1, Offset p2) {
  return Offset((p1.dx + p2.dx) / 2, (p1.dy + p2.dy) / 2);
}

class SnowflakePainter extends CustomPainter {
  final double progress; // Progress of the animation (0.0 to 1.0)

  SnowflakePainter(this.progress);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = Colors.blue
      ..strokeWidth = 3
      ..style = PaintingStyle.stroke;

    final center = Offset(size.width / 2, size.height / 2);
    final radius = size.width / 2;

    // Draw lines for snowflake
    for (int i = 0; i < 6; i++) {
      final angle = (2 * math.pi / 6) * i;
      final lineLength = radius * progress; // Control line length with progress
      final lineEnd =
          center + Offset(math.sin(angle), math.cos(angle)) * lineLength;

      canvas.drawLine(center, lineEnd, paint);

      if (lineLength > radius / 2) {
        final start1 = midpoint(center, lineEnd);
        final angle1 = angle + math.pi / 4;
        final lineLength1 = radius * (progress / 2);
        final end1 =
            start1 + Offset(math.sin(angle1), math.cos(angle)) * lineLength1;
        canvas.drawLine(start1, end1, paint);
      }
      // You can add additional lines to make the snowflake more intricate
    }
  }

  @override
  bool shouldRepaint(SnowflakePainter oldDelegate) =>
      progress != oldDelegate.progress;
}

class SnowflakeDrawingAnimation extends StatefulWidget {
  @override
  _SnowflakeDrawingAnimationState createState() =>
      _SnowflakeDrawingAnimationState();
}

class _SnowflakeDrawingAnimationState extends State<SnowflakeDrawingAnimation>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Duration(seconds: 5),
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, child) {
        return CustomPaint(
          painter: SnowflakePainter(_controller.value),
          size: Size(200, 200),
        );
      },
    );
  }
}
