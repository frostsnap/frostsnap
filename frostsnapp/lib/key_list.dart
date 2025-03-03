import 'package:dotted_border/dotted_border.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/access_structures.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/goal_progress.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/either.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:flutter/material.dart';

typedef KeyItem = Either<FrostKey, RecoverableKey>;

class KeyList extends StatelessWidget {
  final Function(BuildContext, FrostKey) itemBuilder;
  final Function(BuildContext, RecoverableKey) recoverableBuilder;

  const KeyList({
    super.key,
    required this.itemBuilder,
    required this.recoverableBuilder,
  });

  @override
  Widget build(BuildContext context) {
    final keyStateStream = coord.subKeyEvents().toBehaviorSubject().map((
      value,
    ) {
      return value;
    });

    final Stream<List<KeyItem>> keyStream = keyStateStream.map((keyState) {
      return keyState.keys
          .map((frostKey) {
            return KeyItem.left(frostKey);
          })
          .followedBy(
            keyState.recoverable.map((RecoverableKey recoverable) {
              return KeyItem.right(recoverable);
            }),
          )
          .toList();
    });

    return StreamBuilder(
      stream: keyStream,
      builder: (context, snap) {
        final keys = snap.data ?? [];
        if (keys.isEmpty) {
          return Center(child: Text("You don't have any keys"));
        } else {
          return ListView.builder(
            shrinkWrap: true,
            padding: EdgeInsets.symmetric(horizontal: 16.0, vertical: 8.0),
            itemCount: keys.length,
            itemBuilder: (context, index) {
              final key = keys[index];
              return Padding(
                padding: EdgeInsets.only(bottom: 16.0),
                child: key.match(
                  left: (frostKey) {
                    return itemBuilder(context, frostKey);
                  },
                  right: (recoverable) {
                    return recoverableBuilder(context, recoverable);
                  },
                ),
              );
            },
          );
        }
      },
    );
  }
}

class RecoverableKeyCard extends StatelessWidget {
  final RecoverableKey recoverableKey;
  const RecoverableKeyCard({super.key, required this.recoverableKey});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cardTheme = Theme.of(context).cardTheme;
    final ShapeBorder cardShape =
        cardTheme.shape ??
        RoundedRectangleBorder(borderRadius: BorderRadius.circular(12.0));

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
                    ),
                  ],
                ),
                Positioned(
                  top: 8,
                  right: 8,
                  child: ElevatedButton(
                    onPressed: () async {
                      try {
                        coord.startRecovery(
                          keyId: recoverableKey.accessStructureRef.keyId,
                        );
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
  const RecoveringKeyCard({
    super.key,
    this.keyId,
    this.accessStructureSummaries = const [],
    required this.keyName,
  });

  @override
  Widget build(BuildContext context) {
    final cardTheme = Theme.of(context).cardTheme;
    final mainAccessStructure = accessStructureSummaries[0];
    final t = mainAccessStructure.$1;
    final n = mainAccessStructure.$2;
    final ShapeBorder cardShape =
        cardTheme.shape ??
        RoundedRectangleBorder(borderRadius: BorderRadius.circular(12.0));
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
                Row(
                  children: [
                    Text(
                      keyName,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                    SizedBox(width: 8),
                    AccessStructureSummary(t: t, n: n),
                  ],
                ),
                SizedBox(height: 10),
                Text(
                  recoveryText,
                  style: Theme.of(
                    context,
                  ).textTheme.bodySmall!.copyWith(fontStyle: FontStyle.italic),
                ),
                SizedBox(height: 10),
                Row(
                  children: [
                    Expanded(
                      child: AnimatedCustomProgressIndicator(
                        progress: n,
                        total: t,
                      ),
                    ),
                    IconButton(
                      onPressed: () {
                        Navigator.push(
                          context,
                          MaterialPageRoute(
                            builder: (context) {
                              return KeyContext(
                                keyId: keyId!,
                                child: Scaffold(
                                  body: DeleteWalletPage(),
                                  appBar: AppBar(
                                    title: Text("Cancel recovery"),
                                  ),
                                ),
                              );
                            },
                          ),
                        );
                      },
                      icon: Icon(Icons.cancel),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class KeyCard extends StatelessWidget {
  final String keyName;
  final KeyId? keyId;
  late final FrostKey? frostKey;
  final List<(int, int)> accessStructureSummaries;

  KeyCard({
    super.key,
    required this.keyName,
    this.keyId,
    this.accessStructureSummaries = const [],
  }) {
    if (keyId != null) frostKey = coord.getFrostKey(keyId: keyId!);
  }

  Function()? onPressed(BuildContext context) {
    final superWallet = SuperWalletContext.of(context);
    if (superWallet == null || keyId == null || frostKey == null) return null;
    return () => Navigator.push(
      context,
      createRoute(
        superWallet.tryWrapInWalletContext(
          keyId: keyId!,
          child: WalletHomeWithConfetti(),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (t, n) = accessStructureSummaries[0];
    final network = frostKey?.bitcoinNetwork();

    return Card(
      color: theme.colorScheme.secondaryContainer,
      margin: EdgeInsets.all(0.0),
      child: ListTile(
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12.0),
        ),
        onTap: onPressed(context),
        title: Text(keyName),
        subtitle: Row(
          mainAxisSize: MainAxisSize.min,
          children: [AccessStructureSummary(t: t, n: n), Card.outlined()],
        ),
        leading: Badge(
          isLabelVisible: !(network?.isMainnet() ?? true),
          alignment: AlignmentDirectional.bottomStart,
          textColor: theme.colorScheme.error,
          backgroundColor: theme.colorScheme.surface,
          label: Text(network?.name() ?? "", textAlign: TextAlign.center),
          child: CircleAvatar(
            backgroundColor:
                (network?.isMainnet() ?? false)
                    ? theme.colorScheme.primary
                    : theme.colorScheme.error,
            foregroundColor:
                (network?.isMainnet() ?? false)
                    ? theme.colorScheme.onPrimary
                    : theme.colorScheme.onError,
            child: Icon(Icons.currency_bitcoin_rounded),
          ),
        ),
        trailing: Icon(Icons.chevron_right),
        titleTextStyle: theme.textTheme.titleLarge,
        contentPadding: EdgeInsets.symmetric(horizontal: 16.0, vertical: 16.0),
      ),
    );
  }
}

class ActiveAndRecoverableKeyList extends StatelessWidget {
  const ActiveAndRecoverableKeyList({super.key});

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        Positioned.fill(
          child: KeyList(
            itemBuilder: (context, key) {
              final bool isRecovering = key.accessStructureState().field0.every(
                (accs) => switch (accs) {
                  AccessStructureState_Recovering() => true,
                  AccessStructureState_Complete() => false,
                },
              );
              final accessStructureSummaries =
                  key
                      .accessStructureState()
                      .field0
                      .map(
                        (accs) => switch (accs) {
                          AccessStructureState_Recovering(:final field0) => (
                            field0.threshold,
                            field0.gotSharesFrom.length,
                          ),
                          AccessStructureState_Complete(:final field0) => (
                            field0.threshold(),
                            field0.devices().length,
                          ),
                        },
                      )
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
          ),
        ),
      ],
    );
  }
}
