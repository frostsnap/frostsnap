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

class AnimatedCustomProgressIndicator extends StatefulWidget {
  final int progress;
  final int total;

  const AnimatedCustomProgressIndicator({
    Key? key,
    required this.progress,
    required this.total,
  }) : super(key: key);

  @override
  State<AnimatedCustomProgressIndicator> createState() =>
      _AnimatedCustomProgressIndicatorState();
}

class _AnimatedCustomProgressIndicatorState
    extends State<AnimatedCustomProgressIndicator>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _animation;
  double _oldPercentage = 0.0;

  @override
  void initState() {
    super.initState();
    _oldPercentage = (widget.progress / widget.total).clamp(0.0, 1.0);
    _controller = AnimationController(
      vsync: this,
      duration: Duration(milliseconds: 1500),
    );
    _animation = Tween<double>(
      begin: _oldPercentage,
      end: _oldPercentage,
    ).animate(_controller);
  }

  @override
  void didUpdateWidget(covariant AnimatedCustomProgressIndicator oldWidget) {
    super.didUpdateWidget(oldWidget);
    double newPercentage = (widget.progress / widget.total).clamp(0.0, 1.0);
    if (_oldPercentage != newPercentage) {
      _animation = Tween<double>(
        begin: _oldPercentage,
        end: newPercentage,
      ).animate(CurvedAnimation(
        parent: _controller,
        curve: Curves.easeInOut,
      ));
      _oldPercentage = newPercentage;
      _controller.forward(from: 0.0);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(10.0), // Rounded edges
      child: AnimatedBuilder(
        animation: _animation,
        builder: (context, child) {
          return LinearProgressIndicator(
            value: _animation.value,
            minHeight: 8.0, // Thin rectangle shape
            backgroundColor: Colors.grey[300],
            valueColor: AlwaysStoppedAnimation<Color>(Colors.blue),
          );
        },
      ),
    );
  }
}
