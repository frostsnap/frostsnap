import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class DoKeyGenButton extends StatelessWidget {
  final VoidCallback onPressed;
  final String text;

  DoKeyGenButton({required this.onPressed, required this.text});

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onPressed,
      child: Container(
        padding: EdgeInsets.all(16.0),
        color: Colors.blue,
        child: Text(
          text,
          style: TextStyle(
            color: Colors.white,
            fontSize: 16.0,
          ),
        ),
      ),
    );
  }
}

void handleKeygenButtonPressed() {
  global_coordinator.generateNewKey();
}
