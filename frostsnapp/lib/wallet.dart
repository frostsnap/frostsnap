import 'package:flutter/material.dart';

class KeyDisplayPage extends StatefulWidget {
  late String publicKey;

  KeyDisplayPage({required this.publicKey, Key? key}) : super(key: key);

  @override
  _KeyDisplayPageState createState() => _KeyDisplayPageState();
}

class _KeyDisplayPageState extends State<KeyDisplayPage> {
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
              widget.publicKey,
              style: TextStyle(fontSize: 18, color: Colors.blue),
            ),
          ],
        ),
      ),
    );
  }
}
