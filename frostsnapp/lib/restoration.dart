import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/device_discovery.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';

export 'package:frostsnap/restoration/recovery_flow.dart'
    show WalletRecoveryFlow;
export 'package:frostsnap/restoration/device_discovery.dart'
    show RecoveryFlowWithDiscovery;
export 'package:frostsnap/restoration/state.dart';

void continueWalletRecoveryFlowDialog(
  BuildContext context, {
  required RestorationId restorationId,
}) async {
  final homeCtx = HomeContext.of(context);

  final recoveryContext = RecoveryContext.continuingRestoration(
    restorationId: restorationId,
  );

  await MaybeFullscreenDialog.show(
    context: context,
    barrierDismissible: true,
    child: RecoveryFlowWithDiscovery(recoveryContext: recoveryContext),
  );

  await coord.cancelProtocol();
  if (homeCtx == null) {
    return;
  }
  homeCtx.walletListController.selectRecoveringWallet(restorationId);
}
