import 'dart:async';
import 'dart:typed_data';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'hex.dart';

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
        final signatures = await _showSigningProgressDialog(context);
        if (signatures != null) {
          if (context.mounted) {
            Navigator.pop(context);
          }
          await _showSignatureDialog(context, signatures[0]);
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
              return CheckboxListTile(
                title: Text(device.name ?? "<unknown>"),
                value: selected.contains(device.id),
                onChanged: (bool? value) {
                  setState(() {
                    if (value == true) {
                      selected.add(device.id);
                    } else {
                      selected.remove(device.id);
                    }
                  });
                },
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
                          Text(toHex(
                              Uint8List.fromList(signature.field0.toList())))
                        ],
                      ))));
        });
  }

  Future<List<EncodedSignature>?> _showSigningProgressDialog(
      BuildContext context) {
    final signingStream = api
        .startSigning(
            keyId: widget.frostKey.id(),
            devices: selected.toList(),
            message: _messageController.text)
        .asBroadcastStream();
    final devices = widget.frostKey.devices();

    final finishedSigning = signingStream
        .asyncMap(
            (event) => event.whenOrNull(signed: (signatures) => signatures))
        .firstWhere((signatures) => signatures != null);

    final List<Device> selectedDevices = selected
        .map((id) =>
            devices.firstWhere((device) => deviceIdEquals(device.id, id)))
        .toList();

    return showDeviceActionDialog(
        context: context,
        onCancel: () {
          api.cancelAll();
        },
        complete: finishedSigning,
        content: Column(children: [
          Text("Plug in each device"),
          Expanded(
              child: DeviceSigningProgress(
                  stream: signingStream, signers: selectedDevices)),
        ]));
  }
}

class DeviceSigningProgress extends StatelessWidget {
  final List<Device> signers;
  final Stream<CoordinatorToUserSigningMessage> stream;

  DeviceSigningProgress(
      {super.key, required this.stream, required this.signers});

  @override
  Widget build(BuildContext context) {
    final gotShares = deviceIdSet();

    return StreamBuilder(
        initialData: api.deviceListState(),
        stream: deviceListStateStream,
        builder: (context, snapshot) {
          final devicesPluggedIn = deviceIdSet();
          if (snapshot.hasData) {
            devicesPluggedIn
                .addAll(snapshot.data!.devices.map((device) => device.id));
          }
          return StreamBuilder(
              stream: stream,
              builder: (context, snapshot) {
                if (snapshot.hasData) {
                  snapshot.data!
                      .whenOrNull(gotShare: (from) => gotShares.add(from));
                }
                return ListView.builder(
                    itemCount: signers.length,
                    itemBuilder: (context, index) {
                      final device = signers[index];
                      final Widget icon;
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
