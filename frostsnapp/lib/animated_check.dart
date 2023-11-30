import 'package:flutter/material.dart';
import 'dart:math' as math;

import 'package:frostsnapp/device_list_widget.dart';

class CirclePainter extends CustomPainter {
  final double progress;

  CirclePainter(this.progress);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = Colors.green
      ..strokeWidth = 2.0
      ..style = PaintingStyle.stroke;

    canvas.drawArc(
      Rect.fromCenter(
          center: Offset(size.width / 2, size.height / 2),
          width: size.width,
          height: size.height),
      -math.pi / 2, // Start from top
      2 * math.pi * progress, // Sweep angle
      false,
      paint,
    );
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => true;
}

class AnimatedCheckCircle extends StatefulWidget {
  final double size;

  const AnimatedCheckCircle({super.key, this.size = iconSize});

  @override
  _AnimatedCheckCircleState createState() => _AnimatedCheckCircleState();
}

class _AnimatedCheckCircleState extends State<AnimatedCheckCircle>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: 500),
    );

    _controller.forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      alignment: Alignment.center,
      children: [
        Icon(Icons.check, size: widget.size, color: Colors.green),
        AnimatedBuilder(
          animation: _controller,
          builder: (context, child) {
            return CustomPaint(
              painter: CirclePainter(_controller.value),
              size: Size(widget.size, widget.size),
            );
          },
        ),
      ],
    );
  }
}
