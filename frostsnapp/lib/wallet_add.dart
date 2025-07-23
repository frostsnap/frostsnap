import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/keygen.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/wallet_create.dart';

enum AddType { newWallet, recoverWalletWithDevice, recoverWalletWithBackup }

enum VerticalButtonGroupPosition { top, bottom, middle, single }

class WalletAddColumn extends StatelessWidget {
  static const iconSize = 24.0;
  static const cardMargin = EdgeInsets.fromLTRB(16, 4, 16, 4);
  static const cardBorder = BorderRadius.all(Radius.circular(28));
  static const cardBorderTop = BorderRadius.only(
    topLeft: Radius.circular(28),
    topRight: Radius.circular(28),
    bottomLeft: Radius.circular(8),
    bottomRight: Radius.circular(8),
  );
  static const cardBorderBottom = BorderRadius.only(
    topLeft: Radius.circular(8),
    topRight: Radius.circular(8),
    bottomLeft: Radius.circular(28),
    bottomRight: Radius.circular(28),
  );
  static const cardBorderMiddle = BorderRadius.all(Radius.circular(8));
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
        if (showNewToFrostsnap) buildTitle(context, text: 'Create wallet'),
        buildCard(
          context,
          action: () => onPressed(AddType.newWallet),
          emphasize: true,
          isThreeLine: true,
          icon: Icon(Icons.add_rounded, size: iconSize),
          title: 'Create a multi-sig wallet',
          subtitle: 'Set up a secure wallet using multiple Frostsnap devices',
        ),
        buildTitle(
          context,
          text: 'Restore wallet',
          subText: 'Select how you\'d like to provide the first key',
        ),
        buildCard(
          context,
          action: () => onPressed(AddType.recoverWalletWithDevice),
          isThreeLine: true,
          icon: ImageIcon(
            AssetImage('assets/icons/device2.png'),
            size: iconSize,
          ),
          title: 'Existing device',
          subtitle: 'Connect a Frostsnap device that already holds a key',
          groupPosition: VerticalButtonGroupPosition.top,
        ),
        buildCard(
          context,
          action: () => onPressed(AddType.recoverWalletWithBackup),
          isThreeLine: true,
          icon: Icon(Icons.description_outlined, size: iconSize),
          title: 'Load backup',
          subtitle: 'Use a blank Frostsnap device to load in a physical backup',
          groupPosition: VerticalButtonGroupPosition.bottom,
        ),
      ],
    );
  }

  static Widget buildTitle(
    BuildContext context, {
    required String text,
    String? subText,
    bool showInfoIcon = false,
    Widget? trailing,
  }) {
    final theme = Theme.of(context);
    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.symmetric(horizontal: 16).copyWith(top: 12),
      title: Text.rich(
        TextSpan(
          text: text,
          children: showInfoIcon
              ? [
                  TextSpan(text: ' '),
                  WidgetSpan(
                    child: Icon(
                      Icons.info_outline_rounded,
                      size: 16,
                      color: theme.colorScheme.secondary,
                    ),
                  ),
                ]
              : null,
          style: TextStyle(
            color: theme.colorScheme.secondary,
            fontWeight: FontWeight.bold,
          ),
        ),
      ),
      subtitle: subText == null ? null : Text(subText),
      trailing: trailing,
      subtitleTextStyle: theme.textTheme.labelSmall?.copyWith(
        color: theme.colorScheme.secondary,
      ),
    );
  }

  static Widget buildCard(
    BuildContext context, {
    required Widget icon,
    required String title,
    required String subtitle,
    VerticalButtonGroupPosition? groupPosition,
    String? subsubtitle,
    bool emphasize = false,
    bool? isThreeLine,
    Function()? action,
  }) {
    final theme = Theme.of(context);
    final Color? emphasisColor = theme.colorScheme.secondary;
    final Color? onEmphasisColor = theme.colorScheme.onSecondary;
    final Color? color = theme.colorScheme.secondaryContainer;
    final Color? onColor = theme.colorScheme.onSecondaryContainer;

    final listTile = ListTile(
      textColor: emphasize ? onEmphasisColor : onColor,
      iconColor: emphasize ? onEmphasisColor : onColor,
      onTap: action,
      contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      leading: icon,
      trailing: Icon(Icons.chevron_right_rounded),
      title: Text(title),
      isThreeLine: isThreeLine,
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

    return Card.filled(
      color: emphasize ? emphasisColor : color,
      shape: RoundedRectangleBorder(
        borderRadius: switch (groupPosition) {
          null => cardBorder,
          VerticalButtonGroupPosition.top => cardBorderTop,
          VerticalButtonGroupPosition.bottom => cardBorderBottom,
          VerticalButtonGroupPosition.middle => cardBorderMiddle,
          VerticalButtonGroupPosition.single => cardBorder,
        },
      ),
      clipBehavior: Clip.hardEdge,
      margin:
          (groupPosition == VerticalButtonGroupPosition.top ||
              groupPosition == VerticalButtonGroupPosition.middle)
          ? cardMargin.copyWith(bottom: 0)
          : cardMargin,
      child: listTile,
    );
  }

  static void showWalletCreateDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;
    final backupManager = FrostsnapContext.of(context)!.backupManager;

    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: WalletCreatePage(),
    );

    if (!context.mounted || asRef == null) return;
    final accessStructure = coord.getAccessStructure(asRef: asRef)!;
    showWalletCreatedDialog(context, accessStructure);
    homeCtx.openNewlyCreatedWallet(asRef.keyId);

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
