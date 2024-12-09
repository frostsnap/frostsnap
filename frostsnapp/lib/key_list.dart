import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:collection/collection.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:flutter/material.dart';
import 'package:confetti/confetti.dart';
import 'package:rxdart/rxdart.dart';

import 'sign_message.dart';

class KeyList extends StatelessWidget {
  final Function(AccessStructureRef)? onNewKey;
  final Function(BuildContext, FrostKey, BitcoinNetwork?) itemBuilder;

  const KeyList({super.key, this.onNewKey, required this.itemBuilder});

  @override
  Widget build(BuildContext context) {
    final keyStateStream =
        coord.subKeyEvents().toBehaviorSubject().map((value) {
      return value;
    });
    final settingsStream =
        SettingsContext.of(context)!.walletSettings.map((value) {
      return value;
    });

    final showDevicesButton = FilledButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return DeviceSettingsPage();
          }));
        },
        child: Text("Show Devices"));

    final keyStream =
        Rx.combineLatest2(settingsStream, keyStateStream, (settings, keyState) {
      return keyState.keys.map((frostKey) {
        final targetKeyId = frostKey.keyId();
        final BitcoinNetwork network = settings.walletNetworks
                .firstWhereOrNull(
                  (record) => keyIdEquals(record.$1, targetKeyId),
                )
                ?.$2 ??
            BitcoinNetwork.signet(bridge: api);
        return (key: frostKey, network: network);
      }).toList();
    }).map((value) {
      return value;
    });

    final content = StreamBuilder(
        stream: keyStream,
        builder: (context, snap) {
          var keys = [];

          if (snap.hasData) {
            keys = snap.data!;
          }
          final StatelessWidget list;
          if (keys.isEmpty) {
            list = const Text("You don't have any keys yet");
          } else {
            list = ListView.builder(
                shrinkWrap: true,
                itemCount: keys.length,
                itemBuilder: (context, index) {
                  final record = keys[index];
                  return itemBuilder(context, record.key, record.network);
                });
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
                    FilledButton(
                      child: const Text("New key"),
                      onPressed: () async {
                        final newId = await Navigator.push(context,
                            MaterialPageRoute(builder: (context) {
                          return KeyNamePage();
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
  final BitcoinNetwork? bitcoinNetwork;

  const KeyCard({super.key, required this.frostKey, this.bitcoinNetwork});

  @override
  State<KeyCard> createState() => _KeyCard();
}

class _KeyCard extends State<KeyCard> {
  SignTaskDescription? restorableSignSession;

  @override
  void initState() {
    super.initState();
    restorableSignSession =
        coord.persistedSignSessionDescription(keyId: widget.frostKey.keyId());
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final bitcoinNetwork =
        widget.bitcoinNetwork ?? BitcoinNetwork.signet(bridge: api);
    final settingsCtx = SettingsContext.of(context)!;
    final settings = settingsCtx.settings;
    final signButton = FilledButton(
        style:
            FilledButton.styleFrom(backgroundColor: theme.colorScheme.surface),
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return SignMessagePage(frostKey: widget.frostKey);
          }));
        },
        child: Text("Sign"));

    final Widget walletButton = FilledButton(
      style: FilledButton.styleFrom(backgroundColor: theme.colorScheme.surface),
      onPressed: () async {
        final wallet = await settings.loadWallet(network: bitcoinNetwork);
        if (context.mounted) {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return WalletPage(
                masterAppkey: widget.frostKey.masterAppkey(),
                walletName: widget.frostKey.keyName(),
                wallet: wallet);
          }));
        }
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text("₿"),
          if (!bitcoinNetwork.isMainnet())
            Text(
              bitcoinNetwork.name(),
              style:
                  TextStyle(fontSize: 12, color: Colors.red), // Custom styling
            ),
        ],
      ),
    );

    final Widget continueSigning;

    if (restorableSignSession != null) {
      continueSigning = FilledButton(
          style: FilledButton.styleFrom(
              backgroundColor: theme.colorScheme.surface),
          onPressed: () async {
            final signingStream = coord
                .tryRestoreSigningSession(keyId: widget.frostKey.keyId())
                .toBehaviorSubject();

            switch (restorableSignSession!) {
              case SignTaskDescription_Plain(:final message):
                {
                  await signMessageWorkflowDialog(
                      context, signingStream, message);
                }
              case SignTaskDescription_Transaction(:final unsignedTx):
                {
                  final wallet =
                      await settings.loadWallet(network: bitcoinNetwork);

                  if (context.mounted) {
                    await signAndBroadcastWorkflowDialog(
                        wallet: wallet,
                        context: context,
                        signingStream: signingStream,
                        unsignedTx: unsignedTx,
                        masterAppkey: widget.frostKey.masterAppkey());
                  }
                }
            }

            setState(() {
              restorableSignSession = coord.persistedSignSessionDescription(
                  keyId: widget.frostKey.keyId());
            });
          },
          child: Text("Continue signing"));
    } else {
      continueSigning = Container();
    }

    final threshold = widget.frostKey.accessStructures()[0].threshold();

    return Card(
      color: backgroundSecondaryColor,
      child: Padding(
        padding: const EdgeInsets.all(8.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              widget.frostKey.keyName(),
              textAlign: TextAlign.center,
              style: const TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.bold,
                  fontFamily: 'Monospace'),
            ),
            const SizedBox(height: 8),
            Text("Threshold: $threshold",
                style: TextStyle(color: textSecondaryColor)),
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
          itemBuilder: (context, key, network) {
            return KeyCard(frostKey: key, bitcoinNetwork: network);
          },
          onNewKey: (masterAppkey) {
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
