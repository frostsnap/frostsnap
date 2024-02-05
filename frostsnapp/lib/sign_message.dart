import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'package:frostsnapp/global.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'hex.dart';
import "dart:developer" as developer;

class SignMessagePage extends StatelessWidget {
  final FrostKey frostKey;

  const SignMessagePage({super.key, required this.frostKey});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(title: const Text('Sign Message')),
        body: Padding(
          padding: EdgeInsets.all(8.0),
          child: SignMessageForm(
              frostKey: frostKey), // Specify the required number of devices
        ));
  }
}

class SignMessageForm extends StatefulWidget {
  final FrostKey frostKey;

  const SignMessageForm({Key? key, required this.frostKey}) : super(key: key);

  @override
  _SignMessageFormState createState() => _SignMessageFormState();
}

class _SignMessageFormState extends State<SignMessageForm> {
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
    final devices = widget.frostKey.devices();
    final buttonReady = selected.length == widget.frostKey.threshold() &&
        _messageController.text.isNotEmpty;

    var submitButtonOnPressed;
    if (buttonReady) {
      submitButtonOnPressed = () async {
        final signingStream = coord
            .startSigning(
                keyId: widget.frostKey.id(),
                devices: selected.toList(),
                message: _messageController.text)
            .asBroadcastStream();

        final signatures = await signMessageDialog(context, signingStream);
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
        TextField(
          controller: _messageController,
          onChanged: (_) {
            setState(() {});
          },
          decoration: InputDecoration(labelText: 'Message to sign'),
        ),
        SizedBox(height: 20.0),
        Text(
          'Select ${widget.frostKey.threshold()} device${widget.frostKey.threshold() > 1 ? "s" : ""} to sign with:',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 20.0),
        ),
        Expanded(
          child: ListView.builder(
            itemCount: devices.length,
            itemBuilder: (context, index) {
              final device = devices[index];
              final onChanged = (bool? value) {
                setState(() {
                  if (value == true) {
                    selected.add(device.id);
                  } else {
                    selected.remove(device.id);
                  }
                });
              };
              final enoughNonces = coord.noncesAvailable(id: device.id) >= 1;
              return CheckboxListTile(
                title: Text(
                    "${device.name ?? '<unknown>'}${enoughNonces ? '' : ' (not enough nonces)'}"),
                value: selected.contains(device.id),
                onChanged: enoughNonces ? onChanged : null,
              );
            },
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

Future<List<EncodedSignature>?> signMessageDialog(
    BuildContext context, Stream<SigningState> signingStream) async {
  final signatures = await _showSigningProgressDialog(context, signingStream);
  if (signatures != null && context.mounted) {
    await _showSignatureDialog(context, signatures[0]);
  }
  return signatures;
}

Future<List<EncodedSignature>?> _showSigningProgressDialog(
  BuildContext context,
  Stream<SigningState> signingStream,
) {
  final stream = signingStream.asBroadcastStream();

  final finishedSigning = stream
      .asyncMap((event) => event.finishedSignatures)
      .firstWhere((signatures) => signatures.isNotEmpty);

  return showDeviceActionDialog(
      context: context,
      onCancel: () {
        coord.cancelAll();
      },
      complete: finishedSigning,
      content: Column(children: [
        Text("Plug in each device"),
        Expanded(child: DeviceSigningProgress(stream: stream)),
      ]));
}

Future<void> _showSignatureDialog(
    BuildContext context, EncodedSignature signature) {
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
                        Text("Here's your signature"),
                        SizedBox(height: 20),
                        SelectableText(toHex(
                            Uint8List.fromList(signature.field0.toList())))
                      ],
                    ))));
      });
}

class DeviceSigningProgress extends StatelessWidget {
  final Stream<SigningState> stream;

  DeviceSigningProgress({super.key, required this.stream});

  @override
  Widget build(BuildContext context) {
    return StreamBuilder(
        initialData: api.deviceListState(),
        stream: deviceListStateStream,
        builder: (context, snapshot) {
          final devicesPluggedIn = deviceIdSet();
          devicesPluggedIn
              .addAll(snapshot.data!.devices.map((device) => device.id));
          return StreamBuilder<SigningState>(
              initialData: coord.getSigningState()!,
              stream: stream,
              builder: (context, snapshot) {
                if (!snapshot.hasData) {
                  return CircularProgressIndicator();
                }
                final state = snapshot.data!;
                final gotShares = deviceIdSet();
                gotShares.addAll(state.gotShares);
                return ListView.builder(
                    itemCount: state.neededFrom.length,
                    itemBuilder: (context, index) {
                      final Widget icon;
                      final id = state.neededFrom[index];
                      final device = api.getDevice(id: id);
                      if (gotShares.contains(device.id)) {
                        icon = AnimatedCheckCircle();
                      } else if (devicesPluggedIn.contains(device.id)) {
                        icon = Icon(Icons.touch_app,
                            color: Colors.orange, size: iconSize);
                      } else {
                        icon = Icon(Icons.circle_outlined,
                            color: Colors.blue, size: iconSize);
                      }
                      return ListTile(
                        title: Text(device.name ?? "<unknown>"),
                        trailing: Container(height: iconSize, child: icon),
                      );
                    });
              });
        });
  }
}
