import 'package:flutter/material.dart';
import 'package:frostsnap/wallet_add.dart';

enum MethodChoiceKind { startRecovery, continueRecovery, addToWallet }

mixin TitledWidget on Widget {
  String get titleText;
}

class ChooseMethodView extends StatelessWidget with TitledWidget {
  final VoidCallback? onDeviceChosen;
  final VoidCallback? onPhysicalBackupChosen;
  final MethodChoiceKind kind;

  const ChooseMethodView({
    super.key,
    required this.kind,
    this.onDeviceChosen,
    this.onPhysicalBackupChosen,
  });

  @override
  String get titleText => switch (kind) {
    MethodChoiceKind.startRecovery => 'Add the first key',
    MethodChoiceKind.continueRecovery => 'Add another key',
    MethodChoiceKind.addToWallet => 'Add another key',
  };

  @override
  Widget build(BuildContext context) {
    final String subtitle;

    switch (kind) {
      case MethodChoiceKind.startRecovery:
        subtitle =
            'Select how you\'d like to provide the first key for this wallet.';
        break;
      case MethodChoiceKind.continueRecovery:
        subtitle =
            'Select how you\'d like to provide the next key for this wallet.';
        break;

      case MethodChoiceKind.addToWallet:
        subtitle =
            'Select how you\'d like to provide the key for this wallet.\n\n⚠ For now, Frostsnap only supports adding keys that were originally part of the wallet when it was created';
        break;
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        WalletAddColumn.buildTitle(context, text: subtitle),

        WalletAddColumn.buildCard(
          context,
          action: onDeviceChosen,
          icon: ImageIcon(
            AssetImage('assets/icons/device2.png'),
            size: WalletAddColumn.iconSize,
          ),
          title: 'Use existing device',
          subtitle: 'Connect a Frostsnap device that already has a key.',
          groupPosition: VerticalButtonGroupPosition.top,
        ),
        WalletAddColumn.buildCard(
          context,
          action: onPhysicalBackupChosen,
          icon: Icon(
            Icons.description_outlined,
            size: WalletAddColumn.iconSize,
          ),
          title: 'Load from backup',
          subtitle: 'Use a blank Frostsnap device with your physical backup.',
          groupPosition: VerticalButtonGroupPosition.bottom,
        ),
      ],
    );
  }
}
