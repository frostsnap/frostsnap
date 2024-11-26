import 'package:flutter/material.dart';

class GoalProgressIndicator extends StatelessWidget {
  final int goal;
  final int progress;

  const GoalProgressIndicator({
    Key? key,
    required this.goal,
    required this.progress,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    // Ensure progress doesn't exceed the goal
    final int currentProgress = progress.clamp(0, goal);

    return Wrap(
      spacing: 4.0, // Horizontal spacing between boxes
      runSpacing: 4.0, // Vertical spacing between rows
      alignment: WrapAlignment.center, // Center the boxes horizontally
      children: List.generate(goal, (index) {
        final bool isFilled = index < currentProgress;

        return AnimatedContainer(
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeInOut,
          width: 30.0,
          height: 30.0,
          decoration: BoxDecoration(
            color: isFilled ? Colors.green[100] : Colors.grey[200],
            border: Border.all(color: Colors.black26),
            borderRadius: BorderRadius.circular(8.0),
          ),
          child: Center(
            child: Icon(
              isFilled ? Icons.lock_open : Icons.lock_outline,
              color: isFilled ? Colors.green : Colors.grey,
              size: 24.0,
            ),
          ),
        );
      }),
    );
  }
}
