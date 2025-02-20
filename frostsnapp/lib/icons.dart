import 'dart:async';
import 'package:flutter/material.dart';

class SpinningSyncIcon extends StatefulWidget {
  final Stream<bool> spinStream;
  final double? size;
  final IconData? iconData;

  const SpinningSyncIcon({
    super.key,
    required this.spinStream,
    this.iconData,
    this.size,
  });

  factory SpinningSyncIcon.always({double? size, IconData? iconData}) {
    late final StreamController<bool> controller;
    controller = StreamController<bool>.broadcast(
      onListen: () {
        controller.add(true);
      },
    );
    return SpinningSyncIcon(
      spinStream: controller.stream,
      iconData: iconData,
      size: size,
    );
  }

  factory SpinningSyncIcon.until(Future<void> future, {double? size}) {
    late final StreamController<bool> controller;
    controller = StreamController<bool>.broadcast(
      onListen: () {
        controller.add(true);
      },
    );
    future.whenComplete(() => controller.add(false));
    return SpinningSyncIcon(spinStream: controller.stream, size: size);
  }

  @override
  State<SpinningSyncIcon> createState() => _SpinningSyncIconState();
}

class _SpinningSyncIconState extends State<SpinningSyncIcon>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late StreamSubscription<bool> _subscription;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: Duration(seconds: 5),
    );

    _subscription = widget.spinStream.listen(
      (spinning) {
        if (spinning) {
          _startSpinning();
        } else {
          _stopSpinning();
        }
      },
      onDone: () {
        _stopSpinning();
      },
    );
  }

  void _startSpinning() {
    _controller.repeat();
  }

  void _stopSpinning() {
    _controller
        .animateTo(1.0, curve: Curves.linear)
        .then((_) => _controller.stop());
  }

  @override
  void dispose() {
    _subscription.cancel();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, child) {
        return Transform.rotate(
          angle: _controller.value * 2.0 * 3.14159,
          child: Icon(
            widget.iconData ?? Icons.sync,
            size: widget.size ?? IconTheme.of(context).size,
          ),
        );
      },
    );
  }
}

class SpinningSyncButton extends StatefulWidget {
  final SpinningOnPressed onPressed;
  const SpinningSyncButton({super.key, required this.onPressed});
  @override
  State<SpinningSyncButton> createState() => _SpinningSyncButtonState();
}

typedef SpinningOnPressed = Future Function();

class _SpinningSyncButtonState extends State<SpinningSyncButton> {
  final StreamController<bool> _spinController =
      StreamController<bool>.broadcast();
  bool _isAnimating = false;

  @override
  void dispose() {
    _spinController.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IconButton(
      onPressed:
          _isAnimating
              ? null
              : () async {
                setState(() {
                  _isAnimating = true;
                });
                _spinController.add(true);
                await widget.onPressed();
                _spinController.add(false);
                if (mounted) {
                  setState(() {
                    _isAnimating = false;
                  });
                }
              },
      icon: SpinningSyncIcon(spinStream: _spinController.stream),
    );
  }
}
