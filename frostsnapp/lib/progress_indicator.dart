import 'package:flutter/material.dart';

class FsProgressIndicator extends StatelessWidget {
  const FsProgressIndicator({super.key});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
        height: 30.0,
        child: AspectRatio(
            aspectRatio: 1,
            child: CircularProgressIndicator.adaptive(
              valueColor: AlwaysStoppedAnimation<Color>(
                  Theme.of(context).colorScheme.onSurface),
            )));
  }
}
