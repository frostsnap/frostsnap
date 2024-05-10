import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'hex.dart';
import "dart:developer" as developer;

class NostrPage extends StatelessWidget {
  final FrostKey frostKey;

  const NostrPage({super.key, required this.frostKey});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(title: const Text('Nostr')),
        body: Padding(
          padding: EdgeInsets.all(8.0),
          child: SignNostrForm(frostKey: frostKey),
        ));
  }
}

class SignNostrForm extends StatefulWidget {
  final FrostKey frostKey;

  const SignNostrForm({Key? key, required this.frostKey}) : super(key: key);

  @override
  _SignNostrFormState createState() => _SignNostrFormState();
}

class _SignNostrFormState extends State<SignNostrForm> {
  final _messageController = TextEditingController();
  Set<DeviceId> selected = deviceIdSet();

  @override
  void initState() {
    super.initState();
  }

  @override
  void dispose() {
    _messageController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final buttonReady = selected.length == widget.frostKey.threshold() &&
        _messageController.text.isNotEmpty;

    var submitButtonOnPressed;
    if (buttonReady) {
      submitButtonOnPressed = () async {
        final message = _messageController.text;
        final unsignedEvent = await coord.createNostrEvent(
            keyId: widget.frostKey.id(), eventContent: message);
        final signingStream = coord
            .startSigningNostr(
                keyId: widget.frostKey.id(),
                devices: selected.toList(),
                unsignedEvent: unsignedEvent)
            .toBehaviorSubject();

        final signatures =
            await signMessageWorkflowDialog(context, signingStream, message);

        if (signatures != null && context.mounted) {
          final signedEvent =
              unsignedEvent.addSignature(signature: signatures[0]);
          await signedEvent.broadcast();

          await _showNostrSigningDialog(
              context, signatures[0], unsignedEvent.noteId());
        }

        if (signatures != null && context.mounted) {
          Navigator.pop(context);
        }
      };
    }

    return Center(
        child: Container(
            child: Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        SizedBox(height: 20.0),
        Text(
          'npub',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 20.0),
        ),
        GestureDetector(
            onTap: () {
              Clipboard.setData(ClipboardData(
                  text: coord.getNpub(keyId: widget.frostKey.id())));
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text('npub copied to clipboard')),
              );
            },
            child: Text(coord.getNpub(keyId: widget.frostKey.id()))),
        SizedBox(height: 20.0),
        TextField(
          controller: _messageController,
          onChanged: (_) {
            setState(() {});
          },
          decoration: InputDecoration(labelText: 'Post:'),
        ),
        SizedBox(height: 20.0),
        Text(
          'Select ${widget.frostKey.threshold()} device${widget.frostKey.threshold() > 1 ? "s" : ""} to sign with:',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 20.0),
        ),
        Expanded(
          child: SigningDeviceSelector(
            frostKey: widget.frostKey,
            onChanged: (selectedDevices) => setState(() {
              selected = selectedDevices;
            }),
          ),
        ),
        ElevatedButton(
          onPressed: submitButtonOnPressed,
          child: Text('Submit'),
        ),
      ],
    )));
  }
}

Future<void> _showNostrSigningDialog(
    BuildContext context, EncodedSignature signature, String signedDetails) {
  return showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
            title: Text("Signing success"),
            content: Container(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                child: Align(
                    alignment: Alignment.center,
                    child: Column(
                      children: [
                        Text("Note:"),
                        SizedBox(height: 20),
                        GestureDetector(
                            onTap: () {
                              Clipboard.setData(
                                  ClipboardData(text: signedDetails));
                              ScaffoldMessenger.of(context).showSnackBar(
                                SnackBar(
                                    content:
                                        Text('note id copied to clipboard')),
                              );
                            },
                            child: Text(signedDetails)),
                      ],
                    ))));
      });
}
