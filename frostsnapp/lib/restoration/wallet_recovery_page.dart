import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/theme.dart';

class WalletRecoveryPage extends StatelessWidget {
  final RestorationState restorationState;
  final Function(AccessStructureRef) onWalletRecovered;

  const WalletRecoveryPage({
    super.key,
    required this.restorationState,
    required this.onWalletRecovered,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;

    final status = restorationState.status();
    final shareCount = status.shareCount();

    final isRecovered = status.sharedKey != null;
    final hasIncompatibleShares = shareCount.incompatible > 0;

    final sharesNeeded =
        shareCount.needed != null && shareCount.got != null && !isRecovered
        ? shareCount.needed! - shareCount.got!
        : null;
    final isReady = isRecovered && !hasIncompatibleShares;

    final IconData cardIcon;
    final String cardTitle;
    final String cardMessage;
    final Color cardBackgroundColor;
    final Color cardTextColor;

    if (isReady) {
      cardIcon = Icons.check_circle_outline_rounded;
      cardTitle = 'Ready to restore';
      cardMessage =
          "You have enough keys to restore the wallet. You can still continue adding keys now or add them later in the wallet's settings";
      cardBackgroundColor = theme.colorScheme.primaryContainer;
      cardTextColor = theme.colorScheme.onPrimaryContainer;
    } else if (hasIncompatibleShares && isRecovered) {
      cardIcon = Icons.error_outline_rounded;
      cardTitle = 'Remove incompatible shares';
      cardMessage =
          'You have enough compatible keys to restore, but some incompatible shares are present. Remove the incompatible shares before restoring.';
      cardBackgroundColor = theme.colorScheme.errorContainer;
      cardTextColor = theme.colorScheme.onErrorContainer;
    } else if (hasIncompatibleShares) {
      cardIcon = Icons.error_outline_rounded;
      cardTitle = 'Some shares are invalid';
      cardMessage =
          'At least one share is incompatible with the other shares. Try adding more shares.';
      cardBackgroundColor = theme.colorScheme.errorContainer;
      cardTextColor = theme.colorScheme.onErrorContainer;
    } else if (shareCount.needed == null) {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Gathering keys';
      cardMessage = 'Add more keys to restore the wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    } else if (sharesNeeded != null && sharesNeeded > 0) {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Not enough shares';
      cardMessage = sharesNeeded == 1
          ? '1 more key to restore wallet.'
          : '$sharesNeeded more keys needed to restore wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    } else {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Not enough shares';
      cardMessage = 'Add more keys to restore wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    }

    final progressActionCard = Card(
      color: cardBackgroundColor,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(24)),
      child: Padding(
        padding: EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 16,
          children: [
            Icon(cardIcon, color: cardTextColor, size: 24),
            DefaultTextStyle(
              style: theme.textTheme.headlineSmall!.copyWith(
                color: cardTextColor,
              ),
              textAlign: TextAlign.center,
              child: Text(cardTitle),
            ),
            DefaultTextStyle(
              style: theme.textTheme.bodyLarge!.copyWith(color: cardTextColor),
              textAlign: TextAlign.start,
              child: Text(cardMessage),
            ),
            Padding(
              padding: EdgeInsets.only(top: 8),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.end,
                spacing: 8,
                children: [
                  Flexible(
                    child: TextButton.icon(
                      icon: const Icon(Icons.close_rounded),
                      label: const Text('Cancel'),
                      onPressed: () async {
                        if (status.shares.isNotEmpty) {
                          final confirm = await showDialog<bool>(
                            context: context,
                            builder: (context) => AlertDialog(
                              title: Text('Cancel restoration?'),
                              content: Text(
                                'You have ${status.shares.length} key${status.shares.length > 1 ? 's' : ''} added. '
                                'Are you sure you want to cancel this restoration?',
                              ),
                              actions: [
                                TextButton(
                                  onPressed: () =>
                                      Navigator.of(context).pop(false),
                                  child: Text('Keep restoring'),
                                ),
                                TextButton(
                                  onPressed: () =>
                                      Navigator.of(context).pop(true),
                                  child: Text('Cancel restoration'),
                                ),
                              ],
                            ),
                          );
                          if (confirm == true) {
                            coord.cancelRestoration(
                              restorationId: restorationState.restorationId,
                            );
                          }
                        } else {
                          coord.cancelRestoration(
                            restorationId: restorationState.restorationId,
                          );
                        }
                      },
                      style: TextButton.styleFrom(
                        foregroundColor: cardTextColor,
                      ),
                    ),
                  ),
                  Flexible(
                    child: FilledButton.icon(
                      icon: const Icon(Icons.check_rounded),
                      label: const Text('Restore'),
                      onPressed: isReady
                          ? () async {
                              try {
                                final encryptionKey =
                                    await SecureKeyProvider.getEncryptionKey();
                                final accessStructureRef = await coord
                                    .finishRestoring(
                                      restorationId:
                                          restorationState.restorationId,
                                      encryptionKey: encryptionKey,
                                    );

                                onWalletRecovered(accessStructureRef);
                              } catch (e) {
                                if (context.mounted) {
                                  showErrorSnackbar(
                                    context,
                                    "Failed to recover wallet: $e",
                                  );
                                }
                              }
                            }
                          : null,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );

    final appBar = SliverAppBar.medium(
      pinned: true,
      title: Text.rich(
        TextSpan(
          children: [
            TextSpan(text: restorationState.keyName),
            WidgetSpan(
              child: Card.filled(
                color: theme.colorScheme.primaryContainer.withAlpha(80),
                margin: EdgeInsets.only(left: 12, bottom: 2),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 4,
                  ),
                  child: Text(
                    'Wallet in Restoration',
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onPrimaryContainer,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
        overflow: TextOverflow.fade,
        softWrap: false,
      ),
    );

    var devicesColumn = Column(
      spacing: 8,
      children: status.shares.map((share) {
        final deleteButton = IconButton(
          icon: const Icon(Icons.remove_circle_outline),
          tooltip: 'Remove key',
          onPressed: () async {
            if (isRecovered &&
                share.compatibility == ShareCompatibility.compatible) {
              final confirm = await showDialog<bool>(
                context: context,
                builder: (context) => AlertDialog(
                  title: Text('Remove compatible key?'),
                  content: Text(
                    'This key is compatible with your wallet. '
                    'Are you sure you want to remove it?',
                  ),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(false),
                      child: Text('Keep'),
                    ),
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(true),
                      child: Text('Remove'),
                    ),
                  ],
                ),
              );
              if (confirm != true) return;
            }

            await coord.deleteRestorationShare(
              restorationId: restorationState.restorationId,
              deviceId: share.deviceId,
            );
            homeCtx.walletListController.selectRecoveringWallet(
              restorationState.restorationId,
            );
          },
        );
        final deviceName = coord.getDeviceName(id: share.deviceId) ?? '<empty>';
        final showCompatibility = shareCount.incompatible > 0;
        return Card.filled(
          color: theme.colorScheme.surfaceContainerHigh,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(24)),
            side:
                showCompatibility &&
                    share.compatibility == ShareCompatibility.compatible
                ? BorderSide(color: theme.colorScheme.primary, width: 2)
                : BorderSide.none,
          ),
          margin: EdgeInsets.zero,
          child: ListTile(
            contentPadding: EdgeInsets.symmetric(horizontal: 16),
            leading: Icon(Icons.key),
            trailing: deleteButton,
            subtitle: !showCompatibility
                ? null
                : () {
                    final icon;
                    final text;
                    final color;

                    switch (share.compatibility) {
                      case ShareCompatibility.compatible:
                        icon = Icons.check_circle;
                        text = 'Compatible';
                        color = Theme.of(context).colorScheme.primary;
                        break;
                      case ShareCompatibility.incompatible:
                        icon = Icons.cancel;
                        text = 'Incompatible';
                        color = Theme.of(context).colorScheme.error;
                        break;
                      case ShareCompatibility.uncertain:
                        icon = Icons.pending;
                        text = 'Compatibility uncertain';
                        color = Theme.of(context).colorScheme.onSurfaceVariant;
                        break;
                    }

                    return Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(icon, size: 14, color: color),
                        SizedBox(width: 4),
                        Text(
                          text,
                          style: TextStyle(fontSize: 12, color: color),
                        ),
                      ],
                    );
                  }(),
            title: Row(
              spacing: 8,
              children: [
                Flexible(
                  child: Tooltip(
                    message: "key number ${share.index}",
                    child: Text(
                      "#${share.index}",
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                        fontWeight: FontWeight.w500,
                        fontSize: 18,
                      ),
                    ),
                  ),
                ),
                Flexible(
                  child: Text(
                    deviceName,
                    style: monospaceTextStyle.copyWith(fontSize: 18),
                  ),
                ),
              ],
            ),
          ),
        );
      }).toList(),
    );

    final sizeClass = WindowSizeContext.of(context);
    final alignTop =
        sizeClass == WindowSizeClass.compact ||
        sizeClass == WindowSizeClass.medium ||
        sizeClass == WindowSizeClass.expanded;
    return CustomScrollView(
      slivers: [
        appBar,
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16.0),
            child: Align(
              alignment: AlignmentDirectional.center,
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: alignTop ? double.infinity : 600,
                ),
                child: ConstrainedBox(
                  constraints: BoxConstraints(maxWidth: 600),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(4.0),
                        child: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '${shareCount.got ?? "??"}',
                                style: TextStyle(
                                  fontWeight: FontWeight.bold,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                              TextSpan(text: ' out of '),
                              TextSpan(
                                text: '${shareCount.needed ?? "??"}',
                                style: TextStyle(
                                  fontWeight: FontWeight.bold,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                              TextSpan(text: ' keys needed for restoration:'),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(height: 8),
                      devicesColumn,
                      const SizedBox(height: 8),
                      Align(
                        alignment: AlignmentDirectional.centerEnd,
                        child: TextButton.icon(
                          icon: const Icon(Icons.add),
                          label: const Text('Add another key'),
                          onPressed: () {
                            continueWalletRecoveryFlowDialog(
                              context,
                              restorationId: restorationState.restorationId,
                            );
                          },
                        ),
                      ),
                      SizedBox(height: 16),
                      progressActionCard,
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
