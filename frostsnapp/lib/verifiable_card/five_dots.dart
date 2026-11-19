import 'package:flutter/material.dart';

/// The final check's progress: one dot per word, five words (spec §0.6.1.6).
/// The device's gate screen shows the same five dots.
class FiveDots extends StatelessWidget {
  /// How many of the five words are done.
  final int done;

  const FiveDots({super.key, required this.done});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Semantics(
      // "checked" is kept for PASSED in user copy (spec L1.6.5).
      label: '$done of 5 words done',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 10,
        children: List.generate(5, (i) {
          final filled = i < done;
          return AnimatedContainer(
            duration: Durations.medium2,
            width: 12,
            height: 12,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: filled ? theme.colorScheme.primary : Colors.transparent,
              border: Border.all(
                color: filled
                    ? theme.colorScheme.primary
                    : theme.colorScheme.outline,
                width: 1.5,
              ),
            ),
          );
        }),
      ),
    );
  }
}
