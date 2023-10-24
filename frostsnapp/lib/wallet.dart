import 'package:flutter/material.dart';

class KeyDisplayPage extends StatelessWidget {
  final String publicKey;

  const KeyDisplayPage(this.publicKey);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Frost Key'),
      ),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              'Public Key',
              style: TextStyle(fontSize: 24),
            ),
            Text(
              publicKey,
              style: TextStyle(fontSize: 18, color: Colors.blue),
            ),
          ],
        ),
      ),
    );
  }
}
