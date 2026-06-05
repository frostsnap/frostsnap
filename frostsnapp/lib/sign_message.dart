import 'dart:async';
import 'dart:typed_data';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';
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
  Set<DeviceId> selected = <DeviceId>{};

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
  final Set<DeviceId> selected = <DeviceId>{};

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
    final asRef = coord
        .activeSigningSession(sessionId: sessionId!)
        ?.accessStructureRef();
    if (asRef == null) return;
    final encryptionKey = await existingWalletKey(
      context: context.mounted ? context : null,
      accessStructureRef: asRef,
      action: 'sign this message',
    );
    if (encryptionKey == null) return;
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
  Set<DeviceId> selected = <DeviceId>{};
  Stream<SigningState>? _signingStream;
  StreamSubscription<SigningState>? _signingSub;
  FullscreenActionDialogController<void>? _actionDialog;
  SignSessionId? _sessionId;
  bool _finished = false;
  bool _errorHandled = false;

  @override
  void dispose() {
    _messageController.dispose();
    _actionDialog?.dispose();
    if (_signingSub != null && !_finished) {
      _signingSub!.cancel();
      final sessionId = _sessionId;
      if (sessionId != null) coord.cancelSignSession(ssid: sessionId);
      coord.cancelProtocol();
    }
    super.dispose();
  }

  void _startSigning(AccessStructureRef accessStructureRef) {
    final message = _messageController.text;
    final devices = selected.toList();
    _actionDialog = FullscreenActionDialogController<void>(
      context: context,
      devices: devices,
      title: 'Sign message with connected device',
      actionButtons: [
        OutlinedButton(onPressed: _cancel, child: Text('Cancel')),
        DeviceActionHint(),
      ],
      onDismissed: () {},
    );
    final stream = coord
        .startSigningBip322(
          accessStructureRef: accessStructureRef,
          devices: devices,
          message: message,
          addressIndex: widget.address.index,
          external_: widget.address.external,
        )
        .toBehaviorSubject();
    late final StreamSubscription<SigningState> sub;
    sub = stream.listen((state) {
      // Ensure `_onSigningState` is called sequentially.
      sub.pause();
      _onSigningState(
        state,
        accessStructureRef,
        message,
      ).whenComplete(sub.resume);
    }, onError: _onSigningError);
    setState(() {
      _signingStream = stream;
      _signingSub = sub;
    });
  }

  Future<void> _onSigningState(
    SigningState state,
    AccessStructureRef accessStructureRef,
    String message,
  ) async {
    _sessionId = state.sessionId;
    final signatures = state.finishedSignatures;
    if (signatures == null) {
      final encryptionKey = await existingWalletKey(
        context: mounted ? context : null,
        accessStructureRef: accessStructureRef,
        action: 'sign this message',
      );
      if (!mounted) return;
      if (encryptionKey != null) {
        for (final deviceId in state.connectedButNeedRequest) {
          coord.requestDeviceSign(
            deviceId: deviceId,
            sessionId: state.sessionId,
            encryptionKey: encryptionKey,
          );
        }
      }
    }
    await _actionDialog?.batchRemoveActionNeeded(state.gotShares);
    if (signatures == null || !mounted) return;

    _finished = true;
    final encoded = bip322SignatureToString(signature: signatures[0]);
    await _showBip322SignatureDialog(
      context,
      widget.address.address.toString(),
      message,
      encoded,
    );
    if (mounted) Navigator.pop(context);
  }

  void _cancel() async {
    // Dismiss the fullscreen dialog first, otherwise the controller reshows it
    // while a target device is still connected. `dispose()` cancels the session.
    await _actionDialog?.clearAllActionsNeeded();
    if (mounted) Navigator.pop(context);
  }

  void _onSigningError(Object error) {
    if (_errorHandled) return;
    _errorHandled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _actionDialog?.clearAllActionsNeeded();
      if (!mounted) return;
      showErrorSnackbar(
        context,
        'Signing failed: ${displayExceptionMessage(error)}',
      );
      Navigator.pop(context);
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final accessStructure = widget.frostKey.accessStructures()[0];
    final threshold = accessStructure.threshold();
    final signingStream = _signingStream;
    final buttonReady =
        selected.length == threshold && _messageController.text.isNotEmpty;

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
              subtitle: Text('Signing with this address'),
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: TextField(
            controller: _messageController,
            enabled: signingStream == null,
            minLines: 1,
            maxLines: 4,
            onChanged: (_) => setState(() {}),
            decoration: InputDecoration(labelText: 'Message to sign'),
          ),
        ),
        if (signingStream != null) ...[
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ListTile(
                title: Text('Signatures Needed'),
                subtitle: Text('Connect a device to sign'),
              ),
              DeviceSigningProgress(stream: signingStream),
            ],
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: OutlinedButton(onPressed: _cancel, child: Text('Cancel')),
          ),
        ] else ...[
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
              onPressed: buttonReady
                  ? () => _startSigning(accessStructure.accessStructureRef())
                  : null,
              child: Text('Submit'),
            ),
          ),
        ],
      ],
    );
  }
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
            CopyIconButton(data: value, size: 18),
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
        final devicesPluggedIn = snapshot.data!.devices
            .map((device) => device.id)
            .toSet();
        return StreamBuilder<SigningState>(
          stream: stream,
          builder: (context, snapshot) {
            if (!snapshot.hasData) {
              return CircularProgressIndicator();
            }
            final state = snapshot.data!;
            final gotShares = state.gotShares.toSet();
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
