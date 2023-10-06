import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';

class SignMessageButton extends StatelessWidget {
  const SignMessageButton({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
        padding: EdgeInsets.all(16.0),
        child: TextField(
            onSubmitted: handleSignMessagePressed,
            textAlign: TextAlign.center,
            style: TextStyle(fontSize: 30),
            decoration: InputDecoration(
              hintText: "Sign message",
              hintStyle: TextStyle(color: Colors.grey.withOpacity(0.6)),
              border: InputBorder.none,
            )));
  }
}

// class SignMessageScreen extends StatelessWidget {
//   @override
//   Widget build(BuildContext context) {
//     return Scaffold(
//       appBar: AppBar(
//         title: Text('Signing...'),
//       ),
//       body: Center(child: handleSignMessagePressed()),
//     );
//   }
// }

Text handleSignMessagePressed(String message) {
  global_coordinator.signMessage(message);

  return Text("Sign request sent!");
}
