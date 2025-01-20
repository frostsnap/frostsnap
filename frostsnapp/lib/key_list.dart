import 'package:dotted_border/dotted_border.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/access_structures.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/goal_progress.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/either.dart';
import 'package:frostsnapp/wallet_send.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:flutter/material.dart';
import 'package:confetti/confetti.dart';

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

    final showDevicesButton = FilledButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return DeviceSettingsPage();
          }));
        },
        child: Text("Show Devices"));

    final Stream<List<KeyItem>> keyStream = keyStateStream.map((keyState) {
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
                    FilledButton(
                      child: const Text("New wallet"),
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
    final theme = Theme.of(context);
    final cardTheme = Theme.of(context).cardTheme;
    final ShapeBorder cardShape = cardTheme.shape ??
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12.0),
        );

    return Padding(
      padding: const EdgeInsets.all(4.0),
      child: DottedBorder(
        customPath: (size) {
          final Rect rect = Rect.fromLTWH(0, 0, size.width, size.height);
          return cardShape.getOuterPath(rect);
        },
        strokeWidth: 2,
        dashPattern: const [8, 4],
        child: Material(
          color: theme.colorScheme.surfaceContainerLowest,
          shape: cardShape,
          elevation: cardTheme.elevation ?? 1.0,
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Stack(
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    Column(
                      children: [
                        Text(
                          recoverableKey.name,
                          style: Theme.of(context).textTheme.titleMedium,
                          textAlign: TextAlign.center,
                        ),
                        const SizedBox(height: 8),
                        AccessStructureSummary(t: recoverableKey.threshold),
                      ],
                    )
                  ],
                ),
                Positioned(
                  top: 8,
                  right: 8,
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
                    child: const Text("Recover"),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class RecoveringKeyCard extends StatelessWidget {
  final String keyName;
  final KeyId? keyId;
  final List<(int, int)> accessStructureSummaries;
  const RecoveringKeyCard(
      {super.key,
      this.keyId,
      this.accessStructureSummaries = const [],
      required this.keyName});

  @override
  Widget build(BuildContext context) {
    final cardTheme = Theme.of(context).cardTheme;
    final mainAccessStructure = accessStructureSummaries[0];
    final t = mainAccessStructure.$1;
    final n = mainAccessStructure.$2;
    final ShapeBorder cardShape = cardTheme.shape ??
        RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12.0),
        );
    final moreNeeded = t - n;
    String recoveryText = "";
    if (moreNeeded > 1) {
      recoveryText = "$moreNeeded more shares remaining";
    } else if (moreNeeded == 1) {
      recoveryText = "1 more share remaining";
    } else {
      recoveryText = "ready to recover";
    }

    return Padding(
        padding: cardTheme.margin ?? EdgeInsets.all(8.0),
        child: DottedBorder(
            customPath: (size) {
              final Rect rect = Rect.fromLTWH(0, 0, size.width, size.height);
              return cardShape.getOuterPath(rect);
            },
            strokeWidth: 2,
            dashPattern: const [8, 4],
            color: Colors.black, // Customize the border color
            child: Material(
                color: Colors.transparent,
                shadowColor: Colors.transparent,
                shape: cardShape,
                elevation: cardTheme.elevation ?? 1.0,
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
                          AccessStructureSummary(t: t, n: n),
                        ]),
                        SizedBox(height: 10),
                        Text(recoveryText,
                            style: Theme.of(context)
                                .textTheme
                                .bodySmall!
                                .copyWith(fontStyle: FontStyle.italic)),
                        SizedBox(height: 10),
                        Row(children: [
                          Expanded(
                              child: AnimatedCustomProgressIndicator(
                                  progress: n, total: t)),
                          IconButton(
                              onPressed: () {
                                Navigator.push(context,
                                    MaterialPageRoute(builder: (context) {
                                  return KeyContext(
                                      keyId: keyId!,
                                      child: Scaffold(
                                        body: DeleteWalletPage(),
                                        appBar: AppBar(
                                            title: Text("Cancel recovery")),
                                      ));
                                }));
                              },
                              icon: Icon(Icons.cancel))
                        ]),
                        SizedBox(height: 10),
                        Row(
                            mainAxisAlignment: MainAxisAlignment.end,
                            children: [KeyButtons(keyId: keyId!)]),
                      ]),
                ))));
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
    final theme = Theme.of(context);
    final (t, n) = accessStructureSummaries[0];

    return Card(
      color: theme.colorScheme.secondaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              keyName,
              style: Theme.of(context).textTheme.titleLarge,
            ),
            const SizedBox(height: 8),
            AccessStructureSummary(t: t, n: n),
            const SizedBox(height: 8),
            KeyButtons(keyId: keyId!)
          ],
        ),
      ),
    );
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
    final theme = Theme.of(context);
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    final masterAppkey = frostKey?.masterAppkey();
    final bitcoinNetwork = frostKey?.bitcoinNetwork();
    final settingsCtx = SettingsContext.of(context)!;
    final Widget continueSigning;

    final signButton = ElevatedButton(
        onPressed: () {
          if (frostKey != null) {
            Navigator.push(context, MaterialPageRoute(builder: (context) {
              return SignMessagePage(frostKey: frostKey);
            }));
          }
        },
        child: Text("Sign"));

    final Widget walletButton = ElevatedButton(
      onPressed: () async {
        if (frostKey != null) {
          final superWallet = SuperWalletContext.of(context)!;
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return superWallet.tryWrapInWalletContext(
                keyId: api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey!),
                child: WalletHome());
          }));
        }
      },
      child: Badge(
        label: Text(bitcoinNetwork?.name() ?? ""),
        isLabelVisible: !(bitcoinNetwork?.isMainnet() ?? true),
        alignment: AlignmentDirectional.bottomEnd,
        textColor: theme.colorScheme.error,
        backgroundColor: theme.colorScheme.surface,
        child: Icon(Icons.currency_bitcoin),
      ),
    );

    if (restorableSignSession != null && masterAppkey != null) {
      continueSigning = FilledButton(
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
                  final wallet = settingsCtx.loadWallet(keyId: widget.keyId);

                  if (context.mounted && wallet != null) {
                    await signAndBroadcastWorkflowDialog(
                      masterAppkey: wallet.masterAppkey,
                      superWallet: wallet.superWallet,
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

    return Column(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          signButton,
          const SizedBox(width: 5),
          walletButton,
          const SizedBox(width: 5),
          continueSigning,
        ])
      ],
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
            final bool isRecovering = key
                .accessStructureState()
                .field0
                .every((accs) => switch (accs) {
                      AccessStructureState_Recovering() => true,
                      AccessStructureState_Complete() => false,
                    });
            final accessStructureSummaries = key
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
                .toList();

            if (!isRecovering) {
              return KeyCard(
                keyName: key.keyName(),
                keyId: key.keyId(),
                accessStructureSummaries: accessStructureSummaries,
              );
            } else {
              return RecoveringKeyCard(
                keyName: key.keyName(),
                keyId: key.keyId(),
                accessStructureSummaries: accessStructureSummaries,
              );
            }
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
