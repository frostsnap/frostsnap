import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'package:frostsnapp/show_key.dart';
import 'dart:math';

class DoKeyGenButton extends StatefulWidget {
  final int devicecount;

  DoKeyGenButton({required this.devicecount});

  @override
  _DoKeyGenButtonState createState() => _DoKeyGenButtonState();
}

class _DoKeyGenButtonState extends State<DoKeyGenButton> {
  double thresholdSlider = 1;

  @override
  Widget build(BuildContext context) {
    return Visibility(
        visible: widget.devicecount > 0,
        child: Column(
          children: [
            Visibility(
                visible: widget.devicecount >= 2,
                child: Column(children: [
                  Text(
                    'Threshold: ${thresholdSlider.toInt()}',
                    style: TextStyle(fontSize: 18.0),
                  ),
                  Slider(
                      // Force 1 <= threshold <= devicecount
                      value: max(1,
                          min(thresholdSlider, widget.devicecount.toDouble())),
                      onChanged: (newValue) {
                        setState(() {
                          thresholdSlider = newValue;
                        });
                      },
                      divisions: 1,
                      min: 1,
                      max: max(widget.devicecount.toDouble(), 1)),
                ])),
            GestureDetector(
              onTap: () {
                _navigateToKeyGenScreen(context, thresholdSlider.toInt());
              },
              child: Padding(
                padding: EdgeInsets.all(25.0),
                child: Container(
                  padding: EdgeInsets.all(16.0),
                  color: Colors.blue,
                  child: Text(
                    'Generate Key',
                    style: TextStyle(
                      color: Colors.white,
                      fontSize: 16.0,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ));
  }
}

void _navigateToKeyGenScreen(BuildContext context, int threshold) {
  Navigator.of(context).push(
    MaterialPageRoute(
      builder: (context) => DoKeyGenScreen(threshold: threshold),
    ),
  );
}

class DoKeyGenScreen extends StatelessWidget {
  final int threshold;

  const DoKeyGenScreen({Key? key, required this.threshold});

  void keygenConfirmed(BuildContext context, String key) {
    global_coordinator.ackKeygen(true);
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => KeyDisplayPage(key),
      ),
    );
  }

  void keygenRejected(BuildContext context) {
    global_coordinator.ackKeygen(false);
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Key Generation'),
      ),
      body: Center(
        child: FutureBuilder<String>(
          future: handleKeygenButtonPressed(threshold),
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
                  TextButton(
                    onPressed: () {
                      keygenConfirmed(context, '${snapshot.data}');
                    },
                    child: Text("Yes"),
                  ),
                  TextButton(
                    onPressed: () {
                      keygenRejected(context);
                    },
                    child: Text("No"),
                  ),
                ],
              );
            }
          },
        ),
      ),
    );
  }
}

Future<String> handleKeygenButtonPressed(int threshold) {
  return global_coordinator.generateNewKey(threshold);
}
