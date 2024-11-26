import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/goal_progress.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/either.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:flutter/material.dart';
import 'package:confetti/confetti.dart';
import 'package:rxdart/rxdart.dart';

import 'sign_message.dart';

typedef KeyItem = Either<FrostKey, RecoverableKey>;

class KeyList extends StatelessWidget {
  final Function(AccessStructureRef)? onNewKey;
  final Function(BuildContext, FrostKey) itemBuilder;
  final Function(BuildContext, RecoverableKey) recoverableBuilder;

  const KeyList(
      {super.key,
      this.onNewKey,
      required this.itemBuilder,
      required this.recoverableBuilder});

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

    final showDevicesButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return DeviceSettingsPage();
          }));
        },
        child: Text("Show Devices"));

    final Stream<List<KeyItem>> keyStream =
        Rx.combineLatest2(settingsStream, keyStateStream, (settings, keyState) {
      return keyState.keys.map((frostKey) {
        return KeyItem.left(frostKey);
      }).followedBy(keyState.recoverable.map((RecoverableKey recoverable) {
        return KeyItem.right(recoverable);
      })).toList();
    });

    final content = StreamBuilder(
        stream: keyStream,
        builder: (context, snap) {
          var keys = [];

          if (snap.hasData) {
            keys = snap.data!;
          }
          final Widget list;
          if (keys.isEmpty) {
            list = const Center(child: Text("You don't have any keys"));
          } else {
            list = ListView.builder(
                shrinkWrap: true,
                itemCount: keys.length,
                itemBuilder: (context, index) {
                  final key = keys[index];
                  return key.match(left: (frostKey) {
                    return itemBuilder(context, frostKey);
                  }, right: (recoverable) {
                    return recoverableBuilder(context, recoverable);
                  });
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
                    ElevatedButton(
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

class RecoverableKeyCard extends StatelessWidget {
  final RecoverableKey recoverableKey;
  const RecoverableKeyCard({super.key, required this.recoverableKey});

  @override
  Widget build(BuildContext context) {
    return Stack(children: [
      Opacity(
          opacity: 0.5,
          child: Card(
              color: backgroundSecondaryColor,
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Row(children: [
                  Text(
                    recoverableKey.name,
                    style: Theme.of(context).textTheme.titleMedium,
                  )
                ]),
              ))),
      Positioned(
          right: 15,
          top: 20,
          child: ElevatedButton(
              onPressed: () async {
                try {
                  coord.startRecovery(
                      keyId: recoverableKey.accessStructureRef.keyId);
                } on FrbAnyhowException catch (e) {
                  if (context.mounted) {
                    showErrorSnackbarBottom(context, e.anyhow);
                  }
                }
              },
              child: Text("recover")))
    ]);
  }
}

class KeyCard extends StatelessWidget {
  final String keyName;
  final KeyId? keyId;
  final List<(int, int)> accessStructureSummaries;

  const KeyCard(
      {super.key,
      required this.keyName,
      this.keyId,
      this.accessStructureSummaries = const []});

  @override
  Widget build(BuildContext context) {
    final mainAccessStructure = accessStructureSummaries[0];
    final t = mainAccessStructure.$1;
    final n = mainAccessStructure.$2;
    final moreNeeded = t - n;
    String thresholdSummary = "";
    final Widget recoveryProgress;

    if (moreNeeded > 0) {
      String recoveryText = "";
      if (moreNeeded > 1) {
        recoveryText = "$moreNeeded more shares to recover";
      } else if (moreNeeded == 1) {
        recoveryText = "1 more share to recover";
      } else {
        recoveryText = "ready to recover";
      }
      recoveryProgress = OverflowBar(children: [
        GoalProgressIndicator(goal: t, progress: n),
        SizedBox(width: 8),
        Text(recoveryText,
            style: Theme.of(context)
                .textTheme
                .bodySmall!
                .copyWith(fontStyle: FontStyle.italic)),
      ]);
    } else {
      recoveryProgress = SizedBox();
      thresholdSummary = "$t-of-$n";
    }

    return Stack(alignment: Alignment.center, children: [
      Card(
          color: backgroundSecondaryColor,
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Text(
                    keyName,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  SizedBox(width: 8),
                  Text(
                    thresholdSummary,
                    style: Theme.of(context)
                        .textTheme
                        .titleSmall!
                        .copyWith(fontStyle: FontStyle.italic),
                  )
                ]),
                if (moreNeeded > 0) ...[
                  SizedBox(height: 10),
                  recoveryProgress,
                ],
                SizedBox(height: 10),
                Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [KeyButtons(keyId: keyId!)])
              ],
            ),
          )),
      Positioned(
        top: 8,
        right: 8,
        child: IconButton(
          onPressed: () async {
            final settingsCtx = SettingsContext.of(context)!;
            final keyWallet = await settingsCtx.loadKeyWallet(keyId: keyId!);
            if (context.mounted) {
              Navigator.push(context, MaterialPageRoute(builder: (context) {
                Widget page = SettingsPage();
                page = keyWallet != null
                    ? WalletContext(keyWallet: keyWallet, child: page)
                    : KeyContext(
                        keyId: keyId!,
                        child: page,
                      );
                return page;
              }));
            }
          },
          icon: Icon(
            Icons.settings,
          ),
        ),
      ),
    ]);
  }
}

class KeyButtons extends StatefulWidget {
  final KeyId keyId;
  const KeyButtons({super.key, required this.keyId});

  @override
  State<KeyButtons> createState() => _KeyButtons();
}

class _KeyButtons extends State<KeyButtons> {
  SignTaskDescription? restorableSignSession;

  @override
  void initState() {
    super.initState();

    restorableSignSession =
        coord.persistedSignSessionDescription(keyId: widget.keyId);
  }

  @override
  Widget build(BuildContext context) {
    final settingsCtx = SettingsContext.of(context)!;
    final Widget continueSigning;
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    final masterAppkey = frostKey?.masterAppkey();
    final bitcoinNetwork =
        settingsCtx.settings.getWalletNetwork(keyId: widget.keyId);

    if (restorableSignSession != null && masterAppkey != null) {
      continueSigning = ElevatedButton(
          onPressed: () async {
            final signingStream = coord
                .tryRestoreSigningSession(keyId: widget.keyId)
                .toBehaviorSubject();

            switch (restorableSignSession!) {
              case SignTaskDescription_Plain(:final message):
                {
                  await signMessageWorkflowDialog(
                      context, signingStream, message);
                }
              case SignTaskDescription_Transaction(:final unsignedTx):
                {
                  final keyWallet =
                      await settingsCtx.loadKeyWallet(keyId: widget.keyId);

                  if (context.mounted) {
                    await signAndBroadcastWorkflowDialog(
                      keyWallet: keyWallet!,
                      context: context,
                      signingStream: signingStream,
                      unsignedTx: unsignedTx,
                    );
                  }
                }
            }

            setState(() {
              restorableSignSession =
                  coord.persistedSignSessionDescription(keyId: widget.keyId);
            });
          },
          child: Text("Continue signing"));
    } else {
      continueSigning = Container();
    }
    final signButton = ElevatedButton(
        onPressed: masterAppkey == null
            ? null
            : () {
                Navigator.push(context, MaterialPageRoute(builder: (context) {
                  return SignMessagePage(frostKey: frostKey!);
                }));
              },
        child: Text("Sign"));

    final Widget walletButton = ElevatedButton(
      onPressed: masterAppkey == null
          ? null
          : () async {
              final keyWallet = await settingsCtx.loadKeyWallet(
                  keyId:
                      api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey));
              if (context.mounted) {
                Navigator.push(context, MaterialPageRoute(builder: (context) {
                  return WalletPage(keyWallet: keyWallet!);
                }));
              }
            },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text("₿"),
          if (bitcoinNetwork != null && !bitcoinNetwork.isMainnet())
            Text(
              bitcoinNetwork.name(),
              style:
                  TextStyle(fontSize: 12, color: Colors.red), // Custom styling
            ),
        ],
      ),
    );

    return Row(mainAxisAlignment: MainAxisAlignment.center, children: [
      signButton,
      const SizedBox(width: 5),
      walletButton,
      const SizedBox(width: 5),
      continueSigning,
    ]);
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
            return KeyCard(
              keyName: key.keyName(),
              keyId: key.keyId(),
              accessStructureSummaries: key
                  .accessStructureState()
                  .field0
                  .map((accs) => switch (accs) {
                        AccessStructureState_Recovering(:final field0) => (
                            field0.threshold,
                            field0.gotSharesFrom.length
                          ),
                        AccessStructureState_Complete(:final field0) => (
                            field0.threshold(),
                            field0.devices().length
                          ),
                      })
                  .toList(),
            );
          },
          recoverableBuilder: (context, recoverableKey) {
            return RecoverableKeyCard(recoverableKey: recoverableKey);
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
