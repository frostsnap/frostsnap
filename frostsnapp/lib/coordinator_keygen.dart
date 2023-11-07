import 'package:flutter/material.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/coordinator.dart';
import 'dart:math';

class DoKeyGenButton extends StatefulWidget {
  final int namedDevicesCount;

  DoKeyGenButton({required this.namedDevicesCount});

  @override
  _DoKeyGenButtonState createState() => _DoKeyGenButtonState();
}

class _DoKeyGenButtonState extends State<DoKeyGenButton> {
  double thresholdSlider = 1;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Column(children: [
          Text(
            'Threshold: ${thresholdSlider.toInt()}',
            style: TextStyle(fontSize: 18.0),
          ),
          Container(
            width: MediaQuery.of(context).size.width * 0.5,
            child: Slider(
                // Force 1 <= threshold <= devicecount
                value: max(1,
                    min(thresholdSlider, widget.namedDevicesCount.toDouble())),
                onChanged: widget.namedDevicesCount <= 1
                    ? null
                    : (newValue) {
                        setState(() {
                          thresholdSlider = newValue;
                        });
                      },
                divisions: max(widget.namedDevicesCount - 1, 1),
                min: 1,
                max: max(widget.namedDevicesCount.toDouble(), 1)),
          )
        ]),
        ElevatedButton(
            onPressed: widget.namedDevicesCount == 0
                ? null
                : () {
                    int threshold = thresholdSlider.toInt();
                    Navigator.of(context)
                        .pushNamed('/keygen', arguments: threshold);
                  },
            child: Text('Generate Key',
                style: TextStyle(
                  color: Colors.white,
                  fontSize: 16.0,
                ))),
      ],
    );
  }
}

class DoKeyGenScreen extends StatefulWidget {
  final int threshold;

  const DoKeyGenScreen({Key? key, required this.threshold}) : super(key: key);

  @override
  _DoKeyGenScreenState createState() => _DoKeyGenScreenState();
}

class _DoKeyGenScreenState extends State<DoKeyGenScreen> {
  late Future<String> keygenCheck;
  late Future<List<KeygenProgress>> keygenProgress;

  @override
  void initState() {
    super.initState();
    keygenCheck = global_coordinator.generateNewKey(widget.threshold);
    // keygenCheck = global_coordinator.keygenCheck();
    keygenProgress = global_coordinator.keygenProgress();
  }

  @override
  Widget build(BuildContext context) {
    return WillPopScope(
      onWillPop: () async {
        return false; // Prevent back button from doing anything
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text('Key Generation'),
        ),
        body: Center(
          child: FutureBuilder<String?>(
            future: keygenCheck,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return CircularProgressIndicator();
              } else if (snapshot.hasError) {
                return Text('Error: ${snapshot.error}');
              } else {
                return AlertDialog(
                  title: Text(
                    'Does this match on all devices?',
                    style: TextStyle(color: Colors.black),
                  ),
                  content: Text(
                    '${snapshot.data}',
                    style: TextStyle(color: Colors.blue),
                  ),
                  actions: [
                    // TextButton(
                    //   onPressed: () {
                    //     keygenConfirmed(context, '${snapshot.data}');
                    //   },
                    //   child: Text("Yes"),
                    // ),
                    // TextButton(
                    //   onPressed: () {
                    //     keygenRejected(context);
                    //   },
                    //   child: Text("No"),
                    // ),
                  ],
                );
              }
            },
          ),
        ),
      ),
    );
  }
}

// void keygenConfirmed(BuildContext context, String publicKey) {
//   global_coordinator.ackKeygen(true);
//   Navigator.of(context).pop();
//   Navigator.of(context).pushReplacementNamed('/wallet', arguments: publicKey);
// }

Future<void> keygenRejected(BuildContext context) async {
  List<DeviceId> devices = await global_coordinator.registeredDevices();
  for (DeviceId id in devices) {
    global_coordinator.cancel(id);
  }

  // if (await global_coordinator.isAwaitingKeygenAck()) {
  //   global_coordinator.ackKeygen(false);
  // }

  Navigator.of(context).pop();
}
