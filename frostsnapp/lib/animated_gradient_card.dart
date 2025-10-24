import 'package:flutter/material.dart';
import 'package:glowy_borders/glowy_borders.dart';

class AnimatedGradientCard extends StatelessWidget {
  final Widget child;
  final double borderSize;
  final double glowSize;
  final int animationTime;
  final BorderRadius? borderRadius;
  final List<Color>? gradientColors;
  final Color? cardColor;

  const AnimatedGradientCard({
    super.key,
    required this.child,
    this.borderSize = 1.0,
    this.glowSize = 4.0,
    this.animationTime = 6,
    this.borderRadius,
    this.gradientColors,
    this.cardColor,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final effectiveBorderRadius = borderRadius ?? BorderRadius.circular(12.0);
    final effectiveGradientColors =
        gradientColors ??
        [
          theme.colorScheme.outlineVariant,
          theme.colorScheme.primary,
          theme.colorScheme.secondary,
          theme.colorScheme.tertiary,
        ];

    return AnimatedGradientBorder(
      stretchAlongAxis: true,
      borderSize: borderSize,
      glowSize: glowSize,
      animationTime: animationTime,
      borderRadius: effectiveBorderRadius,
      gradientColors: effectiveGradientColors,
      child: Card(margin: EdgeInsets.zero, color: cardColor, child: child),
    );
  }
}

class AnimatedGradientPrompt extends StatelessWidget {
  final Widget icon;
  final Widget content;
  final bool dense;
  final Color? cardColor;

  const AnimatedGradientPrompt({
    super.key,
    required this.icon,
    required this.content,
    this.dense = true,
    this.cardColor,
  });

  @override
  Widget build(BuildContext context) {
    return AnimatedGradientCard(
      cardColor: cardColor,
      child: ListTile(
        dense: dense,
        contentPadding: const EdgeInsets.symmetric(horizontal: 16),
        leading: icon,
        title: content,
      ),
    );
  }
}
