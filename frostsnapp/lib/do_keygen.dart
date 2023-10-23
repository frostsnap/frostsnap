import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';

class DoKeyGenButton extends StatefulWidget {
  final int devicecount;

  DoKeyGenButton({required this.devicecount});

  @override
  _DoKeyGenButtonState createState() => _DoKeyGenButtonState();
}

class _DoKeyGenButtonState extends State<DoKeyGenButton> {
  double sliderValue = 1;

  @override
  Widget build(BuildContext context) {
    return Visibility(
      visible: widget.devicecount > 0,
      child: Column(
        children: [
          Text(
            'Threshold: ${sliderValue.toInt()}',
            style: TextStyle(fontSize: 18.0),
          ),
          Slider(
              value: sliderValue,
              onChanged: (newValue) {
                setState(() {
                  sliderValue = newValue;
                });
              },
              divisions: 1,
              min: 1,
              max: (widget.devicecount.toDouble() > 1)
                  ? widget.devicecount.toDouble()
                  : 1),
          GestureDetector(
            onTap: () {
              Navigator.of(context).push(
                MaterialPageRoute(
                  builder: (context) =>
                      DoKeyGenScreen(threshold: sliderValue.toInt()),
                ),
              );
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
      ),
    );
  }
}

class DoKeyGenScreen extends StatelessWidget {
  final int threshold;

  const DoKeyGenScreen({super.key, required this.threshold});

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
              return Center(
                  child: Card(
                margin: EdgeInsets.all(50.0),
                color: Colors.lightBlueAccent,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    ListTile(
                      title: Text(
                        'Confirm Frost Key',
                        style: TextStyle(
                          fontSize: 25.0,
                        ),
                        textAlign: TextAlign.center,
                      ),
                    ),
                    Padding(
                      padding: EdgeInsets.all(16.0),
                      child: RichText(
                        text: TextSpan(
                          style: TextStyle(
                            fontSize: 18,
                            color: Colors.black,
                          ),
                          children: [
                            TextSpan(
                              text:
                                  'Check this Public Key matches the key shown on each device:\n\n',
                            ),
                            TextSpan(
                              text: '${snapshot.data}',
                              style: TextStyle(
                                color: Colors.white,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ));
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
