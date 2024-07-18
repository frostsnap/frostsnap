import 'dart:typed_data';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/wallet.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/hex.dart';
import 'package:confetti/confetti.dart';

import 'sign_message.dart';

class KeyList extends StatelessWidget {
  final Function(KeyId)? onNewKey;
  final Function(BuildContext, FrostKey) itemBuilder;
  const KeyList({super.key, this.onNewKey, required this.itemBuilder});

  @override
  Widget build(BuildContext context) {
    final keyStateSream = coord.subKeyEvents().toBehaviorSubject();

    final showDevicesButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return DeviceSettingsPage();
          }));
        },
        child: Text("Show Devices"));

    final content = StreamBuilder<KeyState>(
        stream: keyStateSream,
        builder: (context, snap) {
          var keys = [];
          if (snap.hasData) {
            keys = snap.data!.keys;
          }
          final StatelessWidget list;
          if (keys.isEmpty) {
            list = const Text("You don't have any keys yet");
          } else {
            list = ListView.builder(
                shrinkWrap: true,
                itemCount: keys.length,
                itemBuilder: (context, index) =>
                    itemBuilder(context, keys[index]));
          }
          return Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Expanded(child: list),
                const SizedBox(height: 8),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    ElevatedButton(
                      child: const Text("New key"),
                      onPressed: () async {
                        final newId = await Navigator.push(context,
                            MaterialPageRoute(builder: (context) {
                          return const KeyGenPage();
                        }));
                        if (newId != null) {
                          onNewKey?.call(newId);
                        }
                      },
                    ),
                    SizedBox(width: 4),
                    showDevicesButton
                  ],
                ),
              ]);
        });

    return Padding(padding: const EdgeInsets.only(bottom: 20), child: content);
  }
}

class KeyCard extends StatefulWidget {
  final FrostKey frostKey;

  const KeyCard({super.key, required this.frostKey});

  @override
  State<KeyCard> createState() => _KeyCard();
}

class _KeyCard extends State<KeyCard> {
  SignTaskDescription? restorableSignSession;

  @override
  void initState() {
    super.initState();
    restorableSignSession =
        coord.persistedSignSessionDescription(keyId: widget.frostKey.id());
  }

  @override
  Widget build(BuildContext context) {
    final keyId = widget.frostKey.id();
    final signButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return SignMessagePage(frostKey: widget.frostKey);
          }));
        },
        child: Text("Sign"));

    final Widget walletButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return WalletHome(keyId: keyId);
          }));
        },
        child: Text("₿"));

    final continueSigning;

    if (restorableSignSession != null) {
      continueSigning = ElevatedButton(
          onPressed: () async {
            final signingStream = coord
                .tryRestoreSigningSession(keyId: keyId)
                .toBehaviorSubject();

            switch (restorableSignSession!) {
              case SignTaskDescription_Plain(:final message):
                {
                  await signMessageWorkflowDialog(
                      context, signingStream, message);
                }
              case SignTaskDescription_Transaction(:final unsignedTx):
                {
                  await signAndBroadcastWorkflowDialog(
                      context: context,
                      signingStream: signingStream,
                      unsignedTx: unsignedTx,
                      keyId: keyId);
                }
            }

            setState(() {
              restorableSignSession = coord.persistedSignSessionDescription(
                  keyId: widget.frostKey.id());
            });
          },
          child: Text("Continue signing"));
    } else {
      continueSigning = Container();
    }

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(8.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              toHex(Uint8List.fromList(widget.frostKey.id().field0)),
              textAlign: TextAlign.center,
              style: const TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.bold,
                  fontFamily: 'Monospace'),
            ),
            const SizedBox(height: 8),
            Text("Threshold: ${widget.frostKey.threshold()}"),
            Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              signButton,
              const SizedBox(width: 5),
              walletButton,
              const SizedBox(width: 5),
              continueSigning,
            ])
          ],
        ),
      ),
    );
  }
}

class KeyListWithConfetti extends StatefulWidget {
  const KeyListWithConfetti({super.key});

  @override
  State<StatefulWidget> createState() => _KeyListWithConfetti();
}

class _KeyListWithConfetti extends State<KeyListWithConfetti> {
  late ConfettiController _confettiController;

  @override
  void initState() {
    super.initState();
    _confettiController = ConfettiController(duration: Duration(seconds: 2));
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Positioned.fill(
            child: KeyList(
          itemBuilder: (context, key) {
            return KeyCard(frostKey: key);
          },
          onNewKey: (keyId) {
            _confettiController.play();
          },
        )),
        Center(
          child: ConfettiWidget(
              confettiController: _confettiController,
              blastDirectionality: BlastDirectionality.explosive,
              numberOfParticles: 50),
        ),
      ],
    );
  }

  @override
  void dispose() {
    _confettiController.dispose();
    super.dispose();
  }
}
