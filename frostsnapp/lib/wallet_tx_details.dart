import 'dart:async';
import 'dart:io';

import 'package:collection/collection.dart';
import 'package:dynamic_color/dynamic_color.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/psbt.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/psbt_manager.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:url_launcher/url_launcher.dart';

const BROADCAST_TIMEOUT = Duration(seconds: 3);

class TxDetailsModel {
  /// The raw transaction.
  Transaction tx;
  final int chainTipHeight;
  final DateTime now;

  TxDetailsModel({
    required this.tx,
    required this.chainTipHeight,
    required this.now,
  });

  update(Transaction tx) => this.tx = tx;

  int get netValue => tx.balanceDelta() ?? 0;

  /// Number of blocks in our view of the best chain.
  int get chainLength => chainTipHeight + 1;

  /// Number of tx confirmations.
  int get confirmations =>
      chainLength - (tx.confirmationTime?.height ?? chainLength);
  bool get isConfirmed => confirmations > 0;
  bool get isSend => (tx.balanceDelta() ?? 0) < 0;

  /// Human-readable string of the last update. This is either the confirmation time or when we last
  /// saw the tx in the mempool.
  String get lastUpdateString {
    final txTimeRaw = tx.timestamp();
    if (txTimeRaw == null) return 'Not seen yet';
    final txTime = DateTime.fromMillisecondsSinceEpoch(txTimeRaw * 1000);
    return humanReadableTimeDifference(now, txTime);
  }
}

String humanReadableTimeDifference(DateTime currentTime, DateTime itemTime) {
  final Duration difference = currentTime.difference(itemTime);

  if (difference.inSeconds < 60) {
    return 'Just now';
  } else if (difference.inMinutes < 60) {
    return '${difference.inMinutes} minute${difference.inMinutes > 1 ? 's' : ''} ago';
  } else if (difference.inHours < 24) {
    return '${difference.inHours} hour${difference.inHours > 1 ? 's' : ''} ago';
  } else if (difference.inDays == 1) {
    return 'Yesterday';
  } else if (difference.inDays < 7) {
    return '${difference.inDays} day${difference.inDays > 1 ? 's' : ''} ago';
  } else if (difference.inDays < 30) {
    final int weeks = (difference.inDays / 7).floor();
    return '$weeks week${weeks > 1 ? 's' : ''} ago';
  } else if (difference.inDays < 365) {
    final int months = (difference.inDays / 30).floor();
    return '$months month${months > 1 ? 's' : ''} ago';
  } else {
    final int years = (difference.inDays / 365).floor();
    return '$years year${years > 1 ? 's' : ''} ago';
  }
}

bool isSigningDone(SigningState state) =>
    state.gotShares.length >= state.neededFrom.length;

class TxSentOrReceivedTile extends StatelessWidget {
  final TxDetailsModel txDetails;
  final SigningState? signingState;
  final bool hideSubtitle;
  final void Function()? onTap;

  const TxSentOrReceivedTile({
    super.key,
    required this.txDetails,
    this.signingState,
    this.hideSubtitle = false,
    this.onTap,
  });

  bool get signingDone => signingState == null || isSigningDone(signingState!);
  bool get needsBroadcast => txDetails.tx.timestamp() == null;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isSigning = signingState != null;
    final accentColor = isSigning
        ? theme.colorScheme.primary
        : txDetails.isSend
        ? Colors.redAccent.harmonizeWith(theme.colorScheme.primary)
        : Colors.green.harmonizeWith(theme.colorScheme.primary);

    return ListTile(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12.0)),
      contentPadding: EdgeInsets.symmetric(horizontal: 16),
      onTap: onTap,
      title: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Flexible(
            child: Text(
              signingDone
                  ? needsBroadcast
                        ? 'Signed'
                        : txDetails.isSend
                        ? (txDetails.isConfirmed
                              ? 'Confirmed'
                              : 'Confirming...')
                        : (txDetails.isConfirmed ? 'Received' : 'Receiving...')
                  : 'Signing...',
              overflow: TextOverflow.fade,
              softWrap: false,
            ),
          ),
          Expanded(
            flex: 2,
            child: SatoshiText(
              value: txDetails.netValue,
              showSign: true,
              style: theme.textTheme.bodyLarge,
            ),
          ),
        ],
      ),
      subtitle: hideSubtitle
          ? null
          : Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              mainAxisSize: MainAxisSize.max,
              children: [
                Flexible(
                  child: Text(
                    signingDone
                        ? txDetails.lastUpdateString
                        : '${signingState!.neededFrom.length - signingState!.gotShares.length} signatures left',
                    overflow: TextOverflow.fade,
                  ),
                ),
                if (!signingDone || needsBroadcast)
                  Flexible(
                    child: Text(
                      signingDone ? 'Tap to broadcast' : 'Tap to continue',
                      style: TextStyle(color: theme.colorScheme.primary),
                      textAlign: TextAlign.end,
                    ),
                  ),
              ],
            ),
      leading: Badge(
        alignment: AlignmentDirectional.bottomEnd,
        label: Icon(
          isSigning
              ? Icons.key
              : needsBroadcast
              ? Icons.visibility_off
              : Icons.hourglass_top_rounded,
          size: 12.0,
          color: (isSigning || needsBroadcast)
              ? theme.colorScheme.outline
              : theme.colorScheme.onSurface,
        ),
        isLabelVisible: !txDetails.isConfirmed,
        backgroundColor: Colors.transparent,
        child: Icon(
          txDetails.isSend ? Icons.north_east : Icons.south_east,
          color: txDetails.isConfirmed
              ? accentColor
              : (isSigning || needsBroadcast)
              ? theme.colorScheme.outlineVariant
              : theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

class TxDetailsPage extends StatefulWidget {
  final ScrollController? scrollController;
  final TxDetailsModel txDetails;
  final SignSessionId? signingSessionId;
  final SignSessionId? finishedSigningSessionId;
  final AccessStructureRef? accessStructureRef;
  final UnsignedTx? unsignedTx;
  final List<DeviceId>? devices;
  final Stream<TxState> txStates;
  final PsbtManager psbtMan;
  final Psbt? psbt;

  const TxDetailsPage({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required this.psbtMan,
  }) : signingSessionId = null,
       finishedSigningSessionId = null,
       accessStructureRef = null,
       unsignedTx = null,
       devices = null,
       psbt = null;

  const TxDetailsPage.needsBroadcast({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required this.psbtMan,
    required SignSessionId this.finishedSigningSessionId,
  }) : signingSessionId = null,
       accessStructureRef = null,
       unsignedTx = null,
       devices = null,
       psbt = null;

  const TxDetailsPage.restoreSigning({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required this.psbtMan,
    required SignSessionId this.signingSessionId,
  }) : finishedSigningSessionId = null,
       accessStructureRef = null,
       unsignedTx = null,
       devices = null,
       psbt = null;

  const TxDetailsPage.startSigning({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required AccessStructureRef this.accessStructureRef,
    required UnsignedTx this.unsignedTx,
    required List<DeviceId> this.devices,
    required this.psbtMan,
    this.psbt,
  }) : signingSessionId = null,
       finishedSigningSessionId = null;

  bool get isRestoreSigning => signingSessionId != null;
  bool get isStartSigning => accessStructureRef != null && unsignedTx != null;
  bool get isSigning => isRestoreSigning || isStartSigning;

  @override
  State<TxDetailsPage> createState() => _TxDetailsPageState();
}

class _TxDetailsPageState extends State<TxDetailsPage> {
  late TxDetailsModel txDetails;
  SignSessionId? ssid;
  late final StreamSubscription<TxState> txStateSub;
  StreamSubscription<DeviceListUpdate>? devicesSub;
  StreamSubscription<SigningState>? signingSub;
  SigningState? signingState;
  bool? broadcastDone;
  Set<DeviceId> connectedDevices = deviceIdSet([]);
  Psbt? psbt;

  late final actionDialogController;

  bool? get signingDone => signingState == null
      ? null
      : signingState!.gotShares.length >= signingState!.neededFrom.length;

  onTxStateData(TxState data) {
    final tx = data.txs.firstWhereOrNull((tx) => tx.txid == txDetails.tx.txid);
    if (tx != null && mounted) setState(() => txDetails.update(tx));
  }

  bool isFirstRun = true;

  Future<void> onSigningSessionData(SigningState data) async {
    if (!mounted) return;

    if (!widget.isStartSigning) this.isFirstRun = false;

    final hasAllShares = data.gotShares.length >= data.neededFrom.length;

    var psbt = this.psbt;
    if (psbt != null) {
      if (hasAllShares) {
        psbt = txDetails.tx.attachSignaturesToPsbt(
          signatures: data.finishedSignatures,
          psbt: psbt,
        );
        if (psbt == null) {
          showErrorSnackbar(
            context,
            'Failed to attach signatures to PSBT: input ownership mismatch?',
          );
          return;
        }
        showMessageSnackbar(
          context,
          'PSBT signed: ${psbt.serialize().length} bytes',
        );
      }

      if ((widget.isStartSigning && isFirstRun) || hasAllShares) {
        isFirstRun = false;
        widget.psbtMan.insert(ssid: data.sessionId, psbt: psbt);
        if (!hasAllShares) {
          showMessageSnackbar(
            context,
            'PSBT saved: ${psbt.serialize().length} bytes',
          );
        }
      }
    }

    setState(() {
      signingState = data;
      ssid = data.sessionId;
      if (psbt != null) this.psbt = psbt;
    });

    actionDialogController.batchAddActionNeeded(
      context,
      data.connectedButNeedRequest,
    );
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    data.connectedButNeedRequest.forEach(
      (id) => coord.requestDeviceSign(
        deviceId: id,
        sessionId: data.sessionId,
        encryptionKey: encryptionKey,
      ),
    );
    await actionDialogController.batchRemoveActionNeeded(data.gotShares);

    return null;
  }

  onDeviceListData(DeviceListUpdate data) {
    final connectedIds = data.state.devices.map((dev) => dev.id);
    if (mounted) {
      setState(() {
        connectedDevices.clear();
        connectedDevices.addAll(connectedIds);
      });

      // Remove dialogs of devices that are no longer connected.
      actionDialogController.clearAllExcept(connectedIds);
    }
  }

  void _onCancelSigning() {
    if (signingDone ?? false) return;
    Navigator.popUntil(context, (r) => r.isFirst);
  }

  @override
  void initState() {
    super.initState();

    txDetails = widget.txDetails;
    ssid = widget.signingSessionId ?? widget.finishedSigningSessionId;
    psbt = widget.psbt;
    // Attempt to get psbt elsewhere.
    if (psbt == null && ssid != null) {
      psbt = widget.psbtMan.withSsid(ssid: ssid!);
    }
    if (psbt == null) {
      psbt = widget.psbtMan.withTxid(txid: widget.txDetails.tx.rawTxid());
    }

    txStateSub = widget.txStates.listen(onTxStateData);

    actionDialogController = FullscreenActionDialogController(
      title: 'Sign transaction with connected device(s)',
      actionButtons: [
        Builder(
          builder: (context) => OutlinedButton(
            child: Text('Cancel'),
            onPressed: _onCancelSigning,
          ),
        ),
        DeviceActionHint(),
      ],
      onDismissed: () {},
    );

    try {
      if (widget.isSigning) {
        devicesSub = GlobalStreams.deviceListSubject.listen(onDeviceListData);
        broadcastDone = false;
        if (widget.isRestoreSigning) {
          signingSub = coord
              .tryRestoreSigningSession(sessionId: widget.signingSessionId!)
              .listen(onSigningSessionData);
        } else if (widget.isStartSigning) {
          late final StreamSubscription<SigningState> sub;
          sub = coord
              .startSigningTx(
                accessStructureRef: widget.accessStructureRef!,
                unsignedTx: widget.unsignedTx!,
                devices: widget.devices!,
              )
              .listen((state) {
                // Ensure `onSigningSessionData` is called sequentially.
                sub.pause();
                onSigningSessionData(state).whenComplete(sub.resume);
              });
          signingSub = sub;
        }
      }
    } catch (e) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        showErrorSnackbar(context, e.toString());
        Navigator.popUntil(context, (r) => r.isFirst);
      });
    }
  }

  @override
  void dispose() {
    devicesSub?.cancel();
    devicesSub = null;
    if (signingSub?.cancel() != null) {
      coord.cancelProtocol();
      signingSub = null;
    }
    txStateSub.cancel();
    actionDialogController.dispose();
    super.dispose();
  }

  static const margin = EdgeInsets.only(left: 16.0, right: 16.0, bottom: 16.0);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        SliverSafeArea(
          sliver: SliverList(
            delegate: SliverChildListDelegate.fixed([
              Card.filled(
                color: theme.colorScheme.surfaceContainer,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.vertical(
                    top: Radius.circular(24),
                    bottom: Radius.circular(4),
                  ),
                ),
                margin: margin.copyWith(bottom: 2),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8.0),
                  child: TxSentOrReceivedTile(
                    txDetails: txDetails,
                    signingState: signingState,
                    hideSubtitle: true,
                  ),
                ),
              ),
              Card.filled(
                color: theme.colorScheme.surfaceContainer,
                margin: margin,
                clipBehavior: Clip.hardEdge,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.vertical(
                    top: Radius.circular(4),
                    bottom: Radius.circular(24),
                  ),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8.0),
                  child: buildDetailsColumn(
                    context,
                    txDetails: txDetails,
                    dense: true,
                    showConfirmations: !widget.isSigning,
                    signingState: signingState,
                  ),
                ),
              ),
              AnimatedCrossFade(
                firstChild: buildActionsRow(context),
                secondChild: buildSignAndBroadcastCard(context),
                crossFadeState:
                    ((signingDone ?? true) &&
                        (broadcastDone ?? txDetails.tx.timestamp() != null))
                    ? CrossFadeState.showFirst
                    : CrossFadeState.showSecond,
                duration: Durations.medium3,
                sizeCurve: Curves.easeInOutCubicEmphasized,
              ),
            ]),
          ),
        ),
      ],
    );
  }

  Widget buildSignaturesNeededColumn(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      ListTile(
        title: Text('Signatures Needed'),
        subtitle: Text('Connect a device to sign'),
        trailing: Stack(
          alignment: AlignmentDirectional.center,
          children: [
            CircularProgressIndicator(
              value:
                  (signingState?.gotShares.length ?? 0) /
                  (signingState?.neededFrom.length ?? 1),
              backgroundColor: Theme.of(
                context,
              ).colorScheme.surfaceContainerHighest,
              strokeCap: StrokeCap.round,
            ),
            Text(
              '${signingState?.gotShares.length}/${signingState?.neededFrom.length}',
            ),
          ],
        ),
      ),
      ...((signingState?.neededFrom) ?? []).map((deviceId) {
        final deviceName = coord.getDeviceName(id: deviceId) ?? '<no-name>';
        final isConnected = connectedDevices.contains(deviceId);
        final Widget trailing;
        if (signingState!.gotShares.any(
          (gotSharesFrom) => deviceIdEquals(deviceId, gotSharesFrom),
        )) {
          trailing = AnimatedCheckCircle();
        } else {
          trailing = Text(
            isConnected ? 'Requesting Signature' : '',
            style: TextStyle(
              color: isConnected ? Theme.of(context).colorScheme.primary : null,
            ),
          );
        }
        return ListTile(
          enabled: isConnected,
          title: Text(deviceName),
          trailing: trailing,
        );
      }),
      Divider(height: 0.0),
      Align(
        alignment: AlignmentDirectional.centerStart,
        child: Padding(
          padding: EdgeInsets.symmetric(vertical: 4.0, horizontal: 12.0),
          child: TextButton(
            onPressed: () async => showCancelSigningDialog(context),
            style: TextButton.styleFrom(
              foregroundColor: Theme.of(context).colorScheme.error,
            ),
            child: Text('Forget'),
          ),
        ),
      ),
    ],
  );

  Widget buildBroadcastNeededColumn(BuildContext context) {
    final psbt = this.psbt;

    final buttonGroup = Row(
      mainAxisSize: MainAxisSize.min,
      spacing: 8,
      children: [
        if (psbt != null)
          Flexible(
            child: FilledButton.tonal(
              onPressed: () => showExportPsbtDialog(context, psbt),
              child: Text('PSBT'),
            ),
          ),
        Flexible(
          child: FilledButton(
            onPressed: (signingDone ?? true && !isBroadcasting)
                ? () => broadcast(context)
                : null,
            child: Text('Broadcast'),
          ),
        ),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(16.0),
          child: Row(
            spacing: 8.0,
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                flex: 3,
                child: TextButton(
                  onPressed: () async => showCancelBroadcastDialog(context),
                  child: Text('Forget'),
                ),
              ),
              Expanded(child: SizedBox.shrink()),
              buttonGroup,
            ],
          ),
        ),
      ],
    );
  }

  Widget buildSignAndBroadcastCard(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedCrossFade(
      firstChild: AnimatedGradientBorder(
        stretchAlongAxis: true,
        borderSize: 1.0,
        glowSize: 5.0,
        animationTime: 6,
        borderRadius: BorderRadius.circular(12.0),
        gradientColors: [
          theme.colorScheme.outlineVariant,
          theme.colorScheme.primary,
          theme.colorScheme.secondary,
          theme.colorScheme.tertiary,
        ],
        child: (Widget child) {
          final theme = Theme.of(context);
          return Card.filled(
            margin: EdgeInsets.all(0.0),
            color: theme.colorScheme.surfaceContainerHigh,
            child: child,
          );
        }(buildSignaturesNeededColumn(context)),
      ),
      secondChild: Card.filled(
        color: Colors.transparent,
        margin: EdgeInsets.all(0.0),
        child: buildBroadcastNeededColumn(context),
      ),
      crossFadeState: (signingDone ?? true)
          ? CrossFadeState.showSecond
          : CrossFadeState.showFirst,
      duration: Durations.medium3,
      sizeCurve: Curves.easeInOutCubicEmphasized,
    );
  }

  showCancelBroadcastDialog(BuildContext context) async {
    final result = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Cancel Transaction'),
        content: Text('No Bitcoin will be sent.'),
        actionsAlignment: MainAxisAlignment.spaceBetween,
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text('Back'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text('I\'m Sure!'),
          ),
        ],
      ),
    );
    if (result ?? false) {
      if (ssid == null) return;
      await coord.forgetFinishedSignSession(ssid: ssid!);
      if (context.mounted) Navigator.pop(context);
    }
  }

  showCancelSigningDialog(BuildContext context) async {
    final result = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Forget Transaction'),
        content: Text('No Bitcoin will be sent. Transaction will be lost.'),
        actionsAlignment: MainAxisAlignment.spaceBetween,
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text('Back'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text('I\'m Sure!'),
          ),
        ],
      ),
    );
    if (result ?? false) {
      if (ssid == null) return;
      await coord.cancelSignSession(ssid: ssid!);
      if (context.mounted) Navigator.pop(context);
    }
  }

  bool isBroadcasting = false;

  broadcast(BuildContext context) async {
    if (mounted) setState(() => isBroadcasting = true);
    final walletCtx = WalletContext.of(context)!;
    final tx = await txDetails.tx.withSignatures(
      signatures: signingState?.finishedSignatures ?? [],
    );
    var broadcastError = '';
    final broadcasted = await walletCtx.wallet.superWallet
        .broadcastTx(masterAppkey: walletCtx.masterAppkey, tx: tx)
        .timeout(BROADCAST_TIMEOUT)
        .then<bool>(
          (_) => ssid != null,
          onError: (e) {
            broadcastError = e.toString();
            return false;
          },
        );
    if (mounted) {
      if (broadcasted) {
        setState(() {
          isBroadcasting = false;
          broadcastDone = true;
          signingState = null;
          // TODO: For some reason, we are not getting the txState notification properly
          // So we do this manually.
        });
        await Future.delayed(
          Durations.medium1,
          () => onTxStateData(
            walletCtx.wallet.superWallet.txState(
              masterAppkey: walletCtx.masterAppkey,
            ),
          ),
        );
      } else {
        showErrorSnackbar(
          context,
          'Failed to broadcast transaction: $broadcastError',
        );
        setState(() => isBroadcasting = false);
      }
    }
  }

  Widget buildActionsRow(BuildContext context) {
    final psbt = this.psbt;
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Align(
        alignment: AlignmentDirectional.centerEnd,
        child: Wrap(
          spacing: 8.0,
          runSpacing: 8.0,
          alignment: WrapAlignment.end,
          children: [
            if (psbt != null)
              ActionChip(
                avatar: Icon(Icons.description),
                label: Text('Show PSBT'),
                onPressed: () => showExportPsbtDialog(context, psbt),
              ),
            if (!txDetails.isConfirmed && (signingDone ?? true))
              ActionChip(
                avatar: Icon(Icons.publish),
                label: Text('Rebroadcast'),
                onPressed: () async =>
                    await rebroadcastAction(context, txid: txDetails.tx.txid),
              ),
            ActionChip(
              avatar: Icon(Icons.open_in_new),
              label: Text('View in Explorer'),
              onPressed: () async =>
                  await explorerAction(context, txid: txDetails.tx.txid),
            ),
          ],
        ),
      ),
    );
  }
}

Widget buildDetailsColumn(
  BuildContext context, {
  required TxDetailsModel txDetails,
  bool dense = true,
  bool showConfirmations = true,
  SigningState? signingState,
}) {
  const contentPadding = EdgeInsets.symmetric(horizontal: 16);
  final walletCtx = WalletContext.of(context)!;
  final theme = Theme.of(context);
  final fee = txDetails.tx.fee();
  return Column(
    children: [
      if (txDetails.isSend)
        ...txDetails.tx.recipients().where((info) => !info.isMine).map((info) {
          final address = info.address(network: walletCtx.network)?.toString();
          return Column(
            children: [
              ListTile(
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('Recipient #${info.vout}'),
                title: Text(
                  spacedHex(address ?? '<unknown>'),
                  style: monospaceTextStyle,
                  textAlign: TextAlign.end,
                ),
                onTap: address == null
                    ? null
                    : () => copyAction(context, 'Recipient address', address),
              ),
              ListTile(
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('\u2570 Amount'),
                title: SatoshiText(value: info.amount, showSign: false),
                onTap: () =>
                    copyAction(context, 'Recipient amount', '${info.amount}'),
              ),
            ],
          );
        }),
      if (txDetails.isSend)
        ListTile(
          dense: dense,
          contentPadding: contentPadding,
          leading: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('Fee '),
              Card.filled(
                color: theme.colorScheme.surfaceContainerHigh,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6.0,
                    vertical: 2.0,
                  ),
                  child: Text(
                    '${txDetails.tx.feerate()?.toStringAsFixed(1)} sat/vB',
                    style: theme.textTheme.labelSmall,
                  ),
                ),
              ),
            ],
          ),
          title: fee == null ? Text('Unknown') : SatoshiText(value: fee),
          onTap: () => copyAction(context, 'Fee amount', '$fee'),
        ),
      if (showConfirmations)
        ListTile(
          dense: dense,
          contentPadding: contentPadding,
          leading: Text('Confirmations'),
          title: Text(
            txDetails.isConfirmed
                ? '${txDetails.confirmations} Block(s)'
                : 'None',
            textAlign: TextAlign.end,
          ),
          onTap: () => copyAction(
            context,
            'Confirmation count',
            '${txDetails.confirmations}',
          ),
        ),
      ListTile(
        dense: dense,
        contentPadding: contentPadding,
        leading: Text('Txid'),
        title: Text(
          txDetails.tx.txid,
          style: monospaceTextStyle,
          textAlign: TextAlign.end,
        ),
        onTap: () => copyAction(context, 'Txid', txDetails.tx.txid),
      ),
    ],
  );
}

void copyAction(BuildContext context, String what, String data) async {
  await Clipboard.setData(ClipboardData(text: data));
  if (context.mounted)
    showMessageSnackbar(context, '$what copied to clipboard');
}

Future<void> rebroadcastAction(
  BuildContext context, {
  required String txid,
}) async {
  final walletCtx = WalletContext.of(context)!;
  try {
    await walletCtx.superWallet
        .rebroadcast(txid: txid)
        .timeout(BROADCAST_TIMEOUT);
  } catch (e) {
    showErrorSnackbar(context, 'Failed to rebroadcast transaction: $e');
  }
}

Future<void> explorerAction(
  BuildContext context, {
  required String txid,
}) async {
  final walletCtx = WalletContext.of(context)!;
  final explorer = getBlockExplorer(walletCtx.superWallet.network);
  await launchUrl(explorer.replace(path: '${explorer.path}tx/$txid'));
}

void showExportPsbtDialog(BuildContext context, Psbt psbt) async {
  final theme = Theme.of(context).copyWith(
    colorScheme: ColorScheme.fromSeed(
      brightness: Brightness.light,
      seedColor: seedColor,
    ),
  );

  final txid = txidHexString(txid: computeTxidOfPsbt(psbt: psbt));
  final psbtBytes = psbt.serialize();

  final animatedQr = AnimatedQr(input: psbtBytes);
  final saveButton = TextButton(
    onPressed: () async {
      final shortTxid = txid.substring(0, 8);
      final fileName = await FilePicker.platform.saveFile(
        dialogTitle: 'Save PSBT file',
        fileName: 'signed_$shortTxid.psbt',
      );
      if (fileName == null) return;
      final file = File(fileName);
      try {
        await file.writeAsBytes(psbtBytes);
      } catch (e) {
        showErrorSnackbar(context, 'Failed to save PSBT file: $e');
        return;
      }
      Navigator.pop(context);
      showMessageSnackbar(context, 'Saved PSBT file');
    },
    child: Text('Save PSBT'),
  );
  final doneButton = FilledButton(
    onPressed: () => Navigator.pop(context),
    child: Text('Done'),
  );

  await showDialog(
    context: context,
    barrierDismissible: true,
    builder: (context) => Theme(
      data: theme,
      child: Dialog(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: 600),
          child: SingleChildScrollView(
            padding: EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              spacing: 16,
              children: [
                AspectRatio(aspectRatio: 1, child: animatedQr),
                saveButton,
                doneButton,
              ],
            ),
          ),
        ),
      ),
    ),
  );
}
