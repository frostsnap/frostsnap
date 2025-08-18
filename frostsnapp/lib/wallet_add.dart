import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/keygen.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/wallet_create.dart';

enum AddType { newWallet, recoverWalletWithDevice, recoverWalletWithBackup }

class WalletAddColumn extends StatelessWidget {
  static const iconSize = 24.0;
  static const cardMargin = EdgeInsets.fromLTRB(16, 4, 16, 12);
  static const contentPadding = EdgeInsets.symmetric(horizontal: 16);

  final bool showNewToFrostsnap;
  final Function(AddType) onPressed;

  WalletAddColumn({
    super.key,
    this.showNewToFrostsnap = true,
    required this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (showNewToFrostsnap) _buildTitle(context, text: 'New to Frostsnap?'),
        _buildCard(
          context,
          action: () => onPressed(AddType.newWallet),
          emphasize: true,
          icon: Icon(Icons.add_rounded, size: iconSize),
          title: 'Create a new wallet',
          subtitle: 'Set up a secure wallet with new Frostsnap devices',
        ),
        Tooltip(
          triggerMode: TooltipTriggerMode.tap,
          message:
              'Depending on your wallet’s setup, you may need to add more keys to finish recovery.',
          child: _buildTitle(
            context,
            showInfoIcon: true,
            text: 'Start wallet recovery',
          ),
        ),
        _buildCard(
          context,
          action: () => onPressed(AddType.recoverWalletWithDevice),
          icon: ImageIcon(
            AssetImage('assets/icons/device2.png'),
            size: iconSize,
          ),
          title: 'Existing key',
          subtitle: 'Restore with a Frostsnap device',
        ),
        _buildCard(
          context,
          action: () => onPressed(AddType.recoverWalletWithBackup),
          icon: Icon(Icons.description_outlined, size: iconSize),
          title: 'Physical backup',
          subtitle: 'Restore with a recorded key backup',
        ),
      ],
    );
  }

  Widget _buildTitle(
    BuildContext context, {
    required String text,
    String? subText,
    bool showInfoIcon = false,
    Widget? trailing,
  }) {
    final theme = Theme.of(context);
    return ListTile(
      contentPadding: EdgeInsets.symmetric(horizontal: 16),
      title: Text.rich(
        TextSpan(
          text: text,
          children: showInfoIcon
              ? [
                  TextSpan(text: '  '),
                  WidgetSpan(child: Icon(Icons.info_outline_rounded, size: 20)),
                ]
              : null,
        ),
      ),
      subtitle: subText == null ? null : Text(subText),
      trailing: trailing,
      subtitleTextStyle: theme.textTheme.labelSmall,
    );
  }

  Widget _buildCard(
    BuildContext context, {
    required Widget icon,
    required String title,
    required String subtitle,
    String? subsubtitle,
    bool emphasize = false,
    Function()? action,
  }) {
    final theme = Theme.of(context);
    final emphasisColor = theme.colorScheme.primaryContainer;
    final onEmphasisColor = theme.colorScheme.onPrimaryContainer;
    final Color? color = null;
    final Color? onColor = null;

    final listTile = ListTile(
      textColor: emphasize ? onEmphasisColor : onColor,
      iconColor: emphasize ? onEmphasisColor : onColor,
      onTap: action,
      contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      leading: icon,
      trailing: Icon(Icons.chevron_right_rounded),
      title: Text(title),
      subtitle: Text.rich(
        TextSpan(
          text: subtitle,
          children: subsubtitle == null
              ? null
              : [
                  TextSpan(text: '\n'),
                  TextSpan(
                    text: subsubtitle,
                    style: TextStyle(
                      fontStyle: FontStyle.italic,
                      color: theme.colorScheme.outline,
                      fontSize: 13,
                    ),
                  ),
                ],
        ),
      ),
    );

    return emphasize
        ? Card(
            color: emphasize ? emphasisColor : color,
            clipBehavior: Clip.hardEdge,
            margin: cardMargin,
            child: listTile,
          )
        : Card.outlined(
            color: emphasize ? emphasisColor : color,
            clipBehavior: Clip.hardEdge,
            margin: cardMargin,
            child: listTile,
          );
  }

  static void showWalletCreateDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;
    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: WalletCreatePage(),
    );

    if (!context.mounted || asRef == null) return;
    final accessStructure = coord.getAccessStructure(asRef: asRef)!;
    final backupManager = FrostsnapContext.of(context)!.backupManager;

    homeCtx.openNewlyCreatedWallet(asRef.keyId);
    showWalletCreatedDialog(context, accessStructure);
    // Delay this to avoid race condition.
    await Future.delayed(
      Duration(seconds: 1),
      () async =>
          await backupManager.startBackupRun(accessStructure: accessStructure),
    );
  }

  static void showWalletRecoverWithDeviceDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;
    final restorationId = await MaybeFullscreenDialog.show<RestorationId>(
      context: context,
      barrierDismissible: true,
      child: WalletRecoveryFlow.startWithDevice(isDialog: false),
    );
    await coord.cancelProtocol();
    if (restorationId == null) return;
    homeCtx.walletListController.selectRecoveringWallet(restorationId);
  }

  static void showWalletRecoverWithBackupDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;
    final restorationId = await MaybeFullscreenDialog.show<RestorationId>(
      context: context,
      barrierDismissible: true,
      child: WalletRecoveryFlow.startWithPhysicalBackup(isDialog: false),
    );
    await coord.cancelProtocol();
    if (restorationId == null) return;
    homeCtx.walletListController.selectRecoveringWallet(restorationId);
  }
}

Function(AddType) makeOnPressed(BuildContext context) {
  return (addType) {
    switch (addType) {
      case AddType.newWallet:
        WalletAddColumn.showWalletCreateDialog(context);
      case AddType.recoverWalletWithDevice:
        WalletAddColumn.showWalletRecoverWithDeviceDialog(context);
      case AddType.recoverWalletWithBackup:
        WalletAddColumn.showWalletRecoverWithBackupDialog(context);
    }
  };
}
