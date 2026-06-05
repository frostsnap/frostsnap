import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'hex.dart';

class SignMessagePage extends StatelessWidget {
  final FrostKey frostKey;

  const SignMessagePage({super.key, required this.frostKey});

  @override
  Widget build(BuildContext context) {
    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Sign message'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(child: SignMessageForm(frostKey: frostKey)),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );

    return SafeArea(child: scrollView);
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

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 24,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: TextField(
            controller: _messageController,
            onChanged: (_) {
              setState(() {});
            },
            decoration: InputDecoration(labelText: 'Message to sign'),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            'Select $threshold device${threshold > 1 ? "s" : ""} to sign with:',
          ),
        ),
        SigningDeviceSelector(
          frostKey: widget.frostKey,
          onChanged: (selectedDevices) => setState(() {
            selected = selectedDevices;
          }),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: FilledButton(
            onPressed: submitButtonOnPressed,
            child: Text('Submit'),
          ),
        ),
      ],
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
      .firstWhere((signatures) => signatures != null);

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

/// BIP-322 message signing under a specific wallet address.
class Bip322SignPage extends StatelessWidget {
  final FrostKey frostKey;
  final AddressInfo address;

  const Bip322SignPage({
    super.key,
    required this.frostKey,
    required this.address,
  });

  @override
  Widget build(BuildContext context) {
    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Sign message'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(
          child: Bip322SignForm(frostKey: frostKey, address: address),
        ),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );

    return SafeArea(child: scrollView);
  }
}

class Bip322SignForm extends StatefulWidget {
  final FrostKey frostKey;
  final AddressInfo address;

  const Bip322SignForm({
    super.key,
    required this.frostKey,
    required this.address,
  });

  @override
  State<Bip322SignForm> createState() => _Bip322SignFormState();
}

class _Bip322SignFormState extends State<Bip322SignForm> {
  final _messageController = TextEditingController();
  Set<DeviceId> selected = deviceIdSet([]);

  @override
  void dispose() {
    _messageController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final accessStructure = widget.frostKey.accessStructures()[0];
    final threshold = accessStructure.threshold();
    final buttonReady =
        selected.length == threshold && _messageController.text.isNotEmpty;

    Future<void> Function()? submitButtonOnPressed;
    if (buttonReady) {
      submitButtonOnPressed = () async {
        final message = _messageController.text;
        final signingStream = coord
            .startSigningBip322(
              accessStructureRef: accessStructure.accessStructureRef(),
              devices: selected.toList(),
              message: message,
              addressIndex: widget.address.index,
              external_: widget.address.external,
            )
            .toBehaviorSubject();

        await signBip322WorkflowDialog(
          context,
          signingStream,
          message,
          widget.address.address.toString(),
        );
        if (context.mounted) {
          Navigator.pop(context);
        }
      };
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 24,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Card.outlined(
            child: ListTile(
              leading: Text(
                '#${widget.address.index}',
                style: theme.textTheme.labelLarge?.copyWith(
                  color: theme.colorScheme.primary,
                  fontFamily: monospaceTextStyle.fontFamily,
                ),
              ),
              title: Text(
                spacedHex(widget.address.address.toString()),
                style: monospaceTextStyle,
              ),
              subtitle: Text('Signing as this address'),
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: TextField(
            controller: _messageController,
            minLines: 1,
            maxLines: 4,
            onChanged: (_) => setState(() {}),
            decoration: InputDecoration(labelText: 'Message to sign'),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            'Select $threshold device${threshold > 1 ? "s" : ""} to sign with:',
          ),
        ),
        SigningDeviceSelector(
          frostKey: widget.frostKey,
          onChanged: (selectedDevices) => setState(() {
            selected = selectedDevices;
          }),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: FilledButton(
            onPressed: submitButtonOnPressed,
            child: Text('Submit'),
          ),
        ),
      ],
    );
  }
}

Future<bool> signBip322WorkflowDialog(
  BuildContext context,
  Stream<SigningState> signingStream,
  String message,
  String address,
) async {
  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    Text("signing ‘$message’"),
  );
  if (signatures != null && context.mounted) {
    final encoded = bip322SignatureToString(signature: signatures[0]);
    await _showBip322SignatureDialog(context, address, message, encoded);
  }
  return signatures == null;
}

Future<void> _showBip322SignatureDialog(
  BuildContext context,
  String address,
  String message,
  String signature,
) {
  Widget field(String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(label, style: Theme.of(context).textTheme.labelLarge),
            ),
            IconButton(
              tooltip: 'Copy',
              icon: Icon(Icons.copy_rounded, size: 18),
              onPressed: () => Clipboard.setData(ClipboardData(text: value)),
            ),
          ],
        ),
        SelectableText(value, style: monospaceTextStyle),
      ],
    );
  }

  return showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
        title: Text("Signing success"),
        content: SizedBox(
          width: Platform.isAndroid ? double.maxFinite : 400.0,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              spacing: 16,
              children: [
                field('Address', address),
                field('Message', message),
                field('Signature', signature),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text('Done'),
          ),
        ],
      );
    },
  );
}

/// A tool to verify a BIP-322 signed message.
class Bip322VerifyPage extends StatefulWidget {
  const Bip322VerifyPage({super.key});

  @override
  State<Bip322VerifyPage> createState() => _Bip322VerifyPageState();
}

class _Bip322VerifyPageState extends State<Bip322VerifyPage> {
  final _addressController = TextEditingController();
  final _messageController = TextEditingController();
  final _signatureController = TextEditingController();
  bool? _result;

  @override
  void dispose() {
    _addressController.dispose();
    _messageController.dispose();
    _signatureController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final body = Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          Text('Verify a BIP-322 signed message'),
          TextField(
            controller: _addressController,
            decoration: InputDecoration(labelText: 'Address'),
          ),
          TextField(
            controller: _messageController,
            minLines: 1,
            maxLines: 4,
            decoration: InputDecoration(labelText: 'Message'),
          ),
          TextField(
            controller: _signatureController,
            minLines: 1,
            maxLines: 4,
            decoration: InputDecoration(labelText: 'Signature (base64)'),
          ),
          FilledButton(
            onPressed: () {
              final valid = bip322Verify(
                address: _addressController.text.trim(),
                message: _messageController.text,
                signature: _signatureController.text.trim(),
              );
              setState(() => _result = valid);
            },
            child: Text('Verify'),
          ),
          if (_result != null)
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              spacing: 8,
              children: [
                Icon(
                  _result! ? Icons.check_circle : Icons.cancel,
                  color: _result! ? Colors.green : theme.colorScheme.error,
                ),
                Text(
                  _result! ? 'Valid signature' : 'Invalid signature',
                  style: theme.textTheme.titleMedium,
                ),
              ],
            ),
        ],
      ),
    );

    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Verify message'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(child: body),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );
    return SafeArea(child: scrollView);
  }
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
