import 'package:flutter/material.dart';
import 'dart:developer' as developer;

import 'package:frostsnapp/ffi.dart';

class DeviceSetup extends StatelessWidget {
  const DeviceSetup(
      {super.key,
      required this.deviceId,
      this.onSubmitted,
      this.onChanged,
      this.onCancel});

  final DeviceId deviceId;
  final ValueChanged<String>? onSubmitted;
  final ValueChanged<String>? onChanged;
  final Function()? onCancel;

  @override
  Widget build(BuildContext context) {
    bool submitted = false;
    return PopScope(
        onPopInvoked: (didPop) {
          if (!submitted) {
            onCancel?.call();
          }
        },
        child: Scaffold(
          appBar: AppBar(
            title: const Text('Device Setup'),
          ),
          body: Column(
            children: [
              TextField(
                decoration: const InputDecoration(
                  icon: Icon(Icons.person),
                  hintText: 'What do you want name this device?',
                  labelText: 'Name',
                ),
                onSubmitted: (name) {
                  submitted = true;
                  onSubmitted?.call(name);
                },
                onChanged: onChanged,
              ),
            ],
          ),
        ));
  }
}
