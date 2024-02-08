import 'dart:typed_data';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/keygen.dart';
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
    return StreamBuilder<KeyState>(
        initialData: coord.keyState(),
        stream: api.subKeyEvents(),
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
                list,
                const SizedBox(height: 8),
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
                )
              ]);
        });
  }
}

class KeyCard extends StatefulWidget {
  final FrostKey frostKey;

  const KeyCard({super.key, required this.frostKey});

  @override
  State<KeyCard> createState() => _KeyCard();
}

class _KeyCard extends State<KeyCard> {
  bool canContinueSigning = false;

  @override
  void initState() {
    super.initState();
    canContinueSigning =
        coord.canRestoreSigningSession(keyId: widget.frostKey.id());
  }

  @override
  Widget build(BuildContext context) {
    final keyId = widget.frostKey.id();
    final Widget signButton;
    final Widget walletButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return WalletHome(keyId: keyId);
          }));
        },
        child: Text("₿"));

    if (canContinueSigning) {
      signButton = ElevatedButton(
          onPressed: () async {
            final stream = coord
                .tryRestoreSigningSession(keyId: keyId)
                .asBroadcastStream();
            await signMessageDialog(context, stream);
            setState(() {
              canContinueSigning = coord.canRestoreSigningSession(keyId: keyId);
            });
          },
          child: Text("Continue signing"));
    } else {
      signButton = ElevatedButton(
          onPressed: () {
            Navigator.push(context, MaterialPageRoute(builder: (context) {
              return SignMessagePage(frostKey: widget.frostKey);
            }));
          },
          child: Text("Sign"));
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
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 8),
            Text("Threshold: ${widget.frostKey.threshold()}"),
            Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [signButton, const SizedBox(width: 5), walletButton])
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
