import 'dart:async';
import 'dart:typed_data';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'hex.dart';

class SignMessagePage extends StatelessWidget {
  final FrostKey frostKey;

  const SignMessagePage({super.key, required this.frostKey});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: FsAppBar(title: const Text('Sign Message')),
      body: Padding(
        padding: EdgeInsets.all(8.0),
        child: SignMessageForm(
          frostKey: frostKey,
        ), // Specify the required number of devices
      ),
    );
  }
}

class SignMessageForm extends StatefulWidget {
  final FrostKey frostKey;

  const SignMessageForm({super.key, required this.frostKey});

  @override
  State<SignMessageForm> createState() => _SignMessageFormState();
}

class _SignMessageFormState extends State<SignMessageForm> {
  final _messageController = TextEditingController();
  Set<DeviceId> selected = deviceIdSet([]);

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
    final accessStructure = widget.frostKey.accessStructures()[0];
    final threshold = accessStructure.threshold();
    final buttonReady =
        selected.length == threshold && _messageController.text.isNotEmpty;

    Future<void> Function()? submitButtonOnPressed;
    if (buttonReady) {
      submitButtonOnPressed = () async {
        final message = _messageController.text;
        final signingStream = coord
            .startSigning(
              accessStructureRef: accessStructure.accessStructureRef(),
              devices: selected.toList(),
              message: message,
            )
            .toBehaviorSubject();

        await signMessageWorkflowDialog(context, signingStream, message);
        if (context.mounted) {
          Navigator.pop(context);
        }
      };
    }

    return Center(
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
            'Select $threshold device${threshold > 1 ? "s" : ""} to sign with:',
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
      ),
    );
  }
}

class SigningDeviceSelector extends StatefulWidget {
  final FrostKey frostKey;
  final Function(Set<DeviceId>)? onChanged;
  final Iterable<DeviceId>? initialSet;

  const SigningDeviceSelector({
    super.key,
    required this.frostKey,
    this.onChanged,
    this.initialSet,
  });

  @override
  State<SigningDeviceSelector> createState() => _SigningDeviceSelectorState();
}

class _SigningDeviceSelectorState extends State<SigningDeviceSelector> {
  final Set<DeviceId> selected = deviceIdSet([]);

  @override
  void initState() {
    super.initState();
    final initialSet = widget.initialSet;
    if (initialSet != null) selected.addAll(initialSet);
  }

  @override
  void dispose() {
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accessStructure = widget.frostKey.accessStructures()[0];
    final devices = accessStructure.devices();

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: devices.map((id) {
        final name = coord.getDeviceName(id: id);
        onChanged(bool? value) {
          setState(() {
            if (value == true) {
              selected.add(id);
            } else {
              selected.remove(id);
            }
          });
          widget.onChanged?.call(selected);
        }

        final enoughNonces = coord.noncesAvailable(id: id) >= 1;
        return CheckboxListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 16),
          title: Text(
            "${name ?? '<unknown>'}${enoughNonces ? '' : ' (not enough nonces)'}",
          ),
          value: selected.contains(id),
          onChanged: enoughNonces ? onChanged : null,
        );
      }).toList(),
    );
  }
}

Future<bool> signMessageWorkflowDialog(
  BuildContext context,
  Stream<SigningState> signingStream,
  String message,
) async {
  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    Text("signing ‘$message’"),
  );
  if (signatures != null && context.mounted) {
    await _showSignatureDialog(context, signatures[0]);
  }
  return signatures == null;
}

Future<List<EncodedSignature>?> showSigningProgressDialog(
  BuildContext context,
  Stream<SigningState> signingStream,
  Widget description,
) async {
  final stream = signingStream.toBehaviorSubject();
  SignSessionId? sessionId;

  final finishedSigning = stream
      .asyncMap((event) {
        return event.finishedSignatures;
      })
      .firstWhere((signatures) => signatures.isNotEmpty);

  stream.forEach((signingState) async {
    sessionId = signingState.sessionId;
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    for (final deviceId in signingState.connectedButNeedRequest) {
      coord.requestDeviceSign(
        deviceId: deviceId,
        sessionId: sessionId!,
        encryptionKey: encryptionKey,
      );
    }
  });

  final result = await showDeviceActionDialog(
    context: context,
    complete: finishedSigning,
    builder: (context) {
      return Column(
        children: [
          DialogHeader(
            child: Column(
              children: [
                description,
                SizedBox(height: 10),
                Text("plug in each device"),
              ],
            ),
          ),
          DeviceSigningProgress(stream: stream),
        ],
      );
    },
  );

  if (result == null) {
    if (sessionId != null) {
      coord.cancelSignSession(ssid: sessionId!);
    }
    coord.cancelProtocol();
  }
  return result;
}

Future<void> _showSignatureDialog(
  BuildContext context,
  EncodedSignature signature,
) {
  return showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: Text("Signing success"),
        content: SizedBox(
          width: Platform.isAndroid ? double.maxFinite : 400.0,
          child: Align(
            alignment: Alignment.center,
            child: Column(
              children: [
                Text("Here's your signature!"),
                SizedBox(height: 20),
                SelectableText(
                  toHex(Uint8List.fromList(signature.field0.toList())),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}

class DeviceSigningProgress extends StatelessWidget {
  final Stream<SigningState> stream;

  const DeviceSigningProgress({super.key, required this.stream});

  @override
  Widget build(BuildContext context) {
    return StreamBuilder(
      stream: GlobalStreams.deviceListSubject.map((update) => update.state),
      builder: (context, snapshot) {
        final theme = Theme.of(context);
        if (!snapshot.hasData) {
          return CircularProgressIndicator();
        }
        final devicesPluggedIn = deviceIdSet(
          snapshot.data!.devices.map((device) => device.id).toList(),
        );
        return StreamBuilder<SigningState>(
          stream: stream,
          builder: (context, snapshot) {
            if (!snapshot.hasData) {
              return CircularProgressIndicator();
            }
            final state = snapshot.data!;
            final gotShares = deviceIdSet(state.gotShares);
            return ListView.builder(
              physics: NeverScrollableScrollPhysics(),
              shrinkWrap: true,
              itemCount: state.neededFrom.length,
              itemBuilder: (context, index) {
                final Widget icon;
                final id = state.neededFrom[index];
                final name = coord.getDeviceName(id: id);
                if (gotShares.contains(id)) {
                  icon = AnimatedCheckCircle();
                } else if (devicesPluggedIn.contains(id)) {
                  icon = Icon(
                    Icons.touch_app,
                    color: theme.colorScheme.secondary,
                    size: iconSize,
                  );
                } else {
                  icon = Icon(
                    Icons.circle_outlined,
                    color: theme.colorScheme.onSurface,
                    size: iconSize,
                  );
                }
                return ListTile(
                  title: Text(name ?? "<unknown>"),
                  trailing: SizedBox(height: iconSize, child: icon),
                );
              },
            );
          },
        );
      },
    );
  }
}
