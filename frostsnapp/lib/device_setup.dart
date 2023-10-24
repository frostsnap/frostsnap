import 'package:flutter/material.dart';
import 'dart:developer' as developer;

import 'package:frostsnapp/ffi.dart';

class DeviceSetup extends StatelessWidget {
  const DeviceSetup(
      {super.key,
      required this.deviceId,
      this.onSubmitted,
      this.onChanged,
      this.popInvoked});

  final DeviceId deviceId;
  final ValueChanged<String>? onSubmitted;
  final ValueChanged<String>? onChanged;
  final WillPopCallback? popInvoked;

  @override
  Widget build(BuildContext context) {
    return WillPopScope(
        onWillPop: popInvoked,
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
                onSubmitted: onSubmitted,
                onChanged: onChanged,
              ),
            ],
          ),
        ));
  }
}
