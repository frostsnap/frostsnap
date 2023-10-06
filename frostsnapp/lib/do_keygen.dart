import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'package:frostsnapp/sign_message.dart';

class DoKeyGenButton extends StatelessWidget {
  final bool isvisible;
  const DoKeyGenButton({required this.isvisible, super.key});

  @override
  Widget build(BuildContext context) {
    return Visibility(
      visible: isvisible,
      child: GestureDetector(
        onTap: () {
          Navigator.of(context).push(
            MaterialPageRoute(
              builder: (context) => DoKeyGenScreen(),
            ),
          );
        },
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
    );
  }
}

class DoKeyGenScreen extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Key Generation'),
      ),
      body: Center(
        child: FutureBuilder<String>(
          future: handleKeygenButtonPressed(),
          builder: (context, snapshot) {
            if (snapshot.connectionState == ConnectionState.waiting) {
              return CircularProgressIndicator();
            } else if (snapshot.hasError) {
              return Text('Error: ${snapshot.error}');
            } else {
              return Column(children: [
                Text(
                  'Frost Key: ${snapshot.data}',
                  style: TextStyle(
                    fontSize: 20.0,
                  ),
                  textAlign: TextAlign.left,
                ),
                SignMessageButton()
              ]);
            }
          },
        ),
      ),
    );
  }
}

Future<String> handleKeygenButtonPressed() {
  return global_coordinator.generateNewKey();
}
