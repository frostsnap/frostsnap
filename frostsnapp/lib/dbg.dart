import 'dart:math';
import 'package:flutter/material.dart';

/// Debug widget that wraps child in a container with random colored border.
/// Useful for visually inspecting widget boundaries during development.
class Dbg extends StatelessWidget {
  final Widget child;
  final double width;

  const Dbg(this.child, {super.key, this.width = 2.0});

  static final _random = Random();

  static Color _randomColor() {
    return Color.fromRGBO(
      _random.nextInt(256),
      _random.nextInt(256),
      _random.nextInt(256),
      1.0,
    );
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        border: Border.all(color: _randomColor(), width: width),
      ),
      child: child,
    );
  }
}
