import 'dart:async';
import 'dart:io';

import 'package:collection/collection.dart';
import 'package:dynamic_color/dynamic_color.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';
import 'package:frostsnap/psbt.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/psbt_manager.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:url_launcher/url_launcher.dart';

const BROADCAST_TIMEOUT = Duration(seconds: 3);

/// What a signing screen needs, in the two shapes it comes in.
///
/// Both carry the transaction being signed, non-null. A mode enum beside nullable fields put
/// the correlation — start has a transaction, restore has a session — nowhere the compiler
/// could see it, and `!` was how each reader agreed not to check.
sealed class TxSigningParams {
  final UnsignedTx unsignedTx;

  const TxSigningParams({required this.unsignedTx});
}

/// The two states with a signing session still to run. `SigningFinished` has none, which is
/// why `accessStructureRef` and `devices` live here and not on the base.
sealed class TxNeedsSignatures extends TxSigningParams {
  final AccessStructureRef accessStructureRef;
  final List<DeviceId> devices;

  const TxNeedsSignatures({
    required this.accessStructureRef,
    required this.devices,
    required super.unsignedTx,
  });

  Stream<SigningState> startSigning();
}

/// Signing a transaction the caller has just built.
class StartSigning extends TxNeedsSignatures {
  const StartSigning({
    required super.accessStructureRef,
    required super.devices,
    required super.unsignedTx,
  });

  @override
  Stream<SigningState> startSigning() => coord.startSigningTx(
    accessStructureRef: accessStructureRef,
    unsignedTx: unsignedTx,
    devices: devices,
  );
}

/// Reopening a session already in flight.
class RestoreSigning extends TxNeedsSignatures {
  final SignSessionId sessionId;

  const RestoreSigning({
    required super.accessStructureRef,
    required super.devices,
    required super.unsignedTx,
    required this.sessionId,
  });

  /// Recovers everything from the session — devices, access structure, and the transaction
  /// being signed — so no caller has to carry them alongside the id.
  ///
  /// `null` when the session has ended between the caller's check and this call, or is not
  /// signing a bitcoin transaction. Absent data, not a mode.
  static RestoreSigning? of({
    required SignSessionId sessionId,
    required MasterAppkey masterAppkey,
  }) {
    final session = coord.activeSigningSession(sessionId: sessionId);
    if (session == null) return null;
    final unsignedTx = coord.unsignedTxForSession(
      sessionId: sessionId,
      masterAppkey: masterAppkey,
    );
    if (unsignedTx == null) return null;
    return RestoreSigning(
      accessStructureRef: session.accessStructureRef(),
      devices: session.state().neededFrom,
      unsignedTx: unsignedTx,
      sessionId: sessionId,
    );
  }

  @override
  Stream<SigningState> startSigning() =>
      coord.tryRestoreSigningSession(sessionId: sessionId);
}

/// Signing is over and these are the signatures it produced.
///
/// Named for what happened, not for what may follow: a PSBT we signed only part of is in this
/// same state, holding signatures and not ours to broadcast. Whether it can go to the network
/// is `unsignedTx.canBroadcast()`, a property of the transaction rather than of how the screen
/// was opened.
///
/// The signatures are a constructor argument because a screen that offers to broadcast must
/// have something to broadcast. Reading them off the live signing stream is what made
/// broadcasting later fail: a reopened session subscribes to nothing, correctly, so that
/// stream is empty forever while the signatures sit in the coordinator.
class SigningFinished extends TxSigningParams {
  final SignSessionId sessionId;
  final List<EncodedSignature> signatures;

  /// Private so [`of`] is the only way to get one: requiring a *list* is not the same as
  /// requiring signatures, and an empty one reads as ready-to-broadcast right up until the
  /// count check refuses it.
  const SigningFinished._({
    required super.unsignedTx,
    required this.sessionId,
    required this.signatures,
  });

  /// `null` when the session has been forgotten, or was not signing a bitcoin transaction.
  static SigningFinished? of({
    required SignSessionId sessionId,
    required MasterAppkey masterAppkey,
  }) {
    final finished = coord.finishedSigningForSession(
      sessionId: sessionId,
      masterAppkey: masterAppkey,
    );
    if (finished == null || finished.signatures.isEmpty) return null;
    return SigningFinished._(
      unsignedTx: finished.unsignedTx,
      sessionId: sessionId,
      signatures: finished.signatures,
    );
  }
}

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

bool isSigningDone(SigningState state) => state.finishedSignatures != null;

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
                        ? (txDetails.isConfirmed ? 'Sent' : 'Sending...')
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
  final SignSessionId? finishedSigningSessionId;
  final Stream<TxState> txStates;
  final PsbtManager psbtMan;
  final Psbt? psbt;
  final TxSigningParams? signingParams;

  const TxDetailsPage({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required this.psbtMan,
    this.signingParams,
    this.finishedSigningSessionId,
    this.psbt,
  });

  /// Signed and waiting to go to the network. Takes the state rather than only its id: this
  /// is the one screen that can broadcast, and it needs the transaction to do it.
  TxDetailsPage.needsBroadcast({
    super.key,
    this.scrollController,
    required this.txStates,
    required this.txDetails,
    required this.psbtMan,
    required SigningFinished this.signingParams,
  }) : finishedSigningSessionId = signingParams.sessionId,
       psbt = null;

  bool get isSigning => signingParams is TxNeedsSignatures;

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

  FullscreenActionDialogController<void>? actionDialogController;

  FullscreenActionDialogController<void> _buildActionDialogController(
    List<DeviceId> devices,
  ) {
    return FullscreenActionDialogController<void>(
      context: context,
      devices: devices,
      title: 'Sign transaction with connected device',
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
  }

  bool get signingDone => signingState == null || isSigningDone(signingState!);

  onTxStateData(TxState data) {
    final tx = data.txs.firstWhereOrNull((tx) => tx.txid == txDetails.tx.txid);
    if (tx != null && mounted) setState(() => txDetails.update(tx));
  }

  bool isFirstRun = true;

  Future<void> onSigningSessionData(SigningState data) async {
    if (!mounted) return;

    // This only runs on a signing session's stream, so the params are there for the whole
    // body. Naming them once is what lets everything below read them without asserting.
    // This only runs on a signing session's stream, so the params are there and are one of
    // the two states that have one. Naming them once is what lets everything below read them
    // without asserting.
    final signingParams = widget.signingParams;
    if (signingParams is! TxNeedsSignatures) return;

    if (signingParams is! StartSigning) this.isFirstRun = false;

    final signatures = data.finishedSignatures;

    var psbt = this.psbt;
    if (psbt != null) {
      if (signatures != null) {
        // The session that produced the signatures is what knows which input each belongs
        // to; the transaction on screen is only a view of it.
        psbt = signingParams.unsignedTx.attachSignaturesToPsbt(
          signatures: signatures,
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

      if ((widget.signingParams is StartSigning && isFirstRun) ||
          signatures != null) {
        isFirstRun = false;
        widget.psbtMan.insert(ssid: data.sessionId, psbt: psbt);
        if (signatures == null) {
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

    final encryptionKey = await existingWalletKey(
      context: mounted ? context : null,
      accessStructureRef: signingParams.accessStructureRef,
      action: 'sign this transaction',
    );
    if (encryptionKey != null) {
      data.connectedButNeedRequest.forEach(
        (id) => coord.requestDeviceSign(
          deviceId: id,
          sessionId: data.sessionId,
          encryptionKey: encryptionKey,
        ),
      );
    }
    await actionDialogController?.batchRemoveActionNeeded(data.gotShares);

    return null;
  }

  onDeviceListData(DeviceListUpdate data) {
    final connectedIds = data.state.devices.map((dev) => dev.id);
    if (mounted) {
      setState(() {
        connectedDevices.clear();
        connectedDevices.addAll(connectedIds);
      });
    }
  }

  void _onCancelSigning() async {
    if (signingDone) return;
    // Dismiss the fullscreen sign dialog first — otherwise the controller
    // would reshow it as soon as it notices a target device still connected.
    // Then pop this page; `dispose()` handles the Rust-side protocol cancel.
    await actionDialogController?.clearAllActionsNeeded();
    if (!mounted) return;
    Navigator.pop(context);
  }

  @override
  void initState() {
    super.initState();

    txDetails = widget.txDetails;
    ssid = widget.finishedSigningSessionId;
    psbt = widget.psbt;
    // Attempt to get psbt elsewhere.
    if (psbt == null && ssid != null) {
      psbt = widget.psbtMan.withSsid(ssid: ssid!);
    }
    if (psbt == null) {
      psbt = widget.psbtMan.withTxid(txid: widget.txDetails.tx.rawTxid());
    }

    txStateSub = widget.txStates.listen(onTxStateData);

    try {
      final signingParams = widget.signingParams;
      switch (signingParams) {
        case TxNeedsSignatures():
          // `devices` is invariant for both start and restore — for restore we
          // hydrated it synchronously from the active session. Seed the dialog
          // controller up front so we never go through the lazy / nullable
          // pattern mid-stream.
          actionDialogController = _buildActionDialogController(
            signingParams.devices,
          );
          devicesSub = GlobalStreams.deviceListSubject.listen(onDeviceListData);
          broadcastDone = false;
          late final StreamSubscription<SigningState> sub;
          sub = signingParams.startSigning().listen(
            (state) {
              // Ensure `onSigningSessionData` is called sequentially.
              sub.pause();
              onSigningSessionData(state).whenComplete(sub.resume);
            },
            // A session that failed to start arrives here rather than at the
            // call: frb drops the `Result` of a stream-returning call, so rust
            // puts the failure onto the stream instead. Without this the error
            // is an unhandled zone error and the screen sits on a session that
            // never began.
            onError: (error) {
              if (!mounted) return;
              showErrorSnackbar(context, error.toString());
              Navigator.popUntil(context, (r) => r.isFirst);
            },
          );
          signingSub = sub;
        case SigningFinished():
          // Signing is over; there is no stream to follow, only a transaction to send.
          broadcastDone = false;
        case null:
          break;
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
    actionDialogController?.dispose();
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
                    (signingDone &&
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

  Widget buildSignaturesNeededColumn(BuildContext context) {
    final theme = Theme.of(context);
    final params = widget.signingParams;
    final asRef = params is TxNeedsSignatures
        ? params.accessStructureRef
        : null;
    final accessStruct = asRef != null
        ? coord.getAccessStructure(asRef: asRef)
        : null;
    return Column(
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
                backgroundColor: theme.colorScheme.surfaceContainerHighest,
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
          final shareIndex = accessStruct?.getDeviceShortShareIndex(
            deviceId: deviceId,
          );
          final label = shareIndex != null
              ? '#$shareIndex $deviceName'
              : deviceName;
          final Widget trailing;
          if (signingState!.gotShares.any(
            (gotSharesFrom) => deviceIdEquals(deviceId, gotSharesFrom),
          )) {
            trailing = AnimatedCheckCircle();
          } else {
            trailing = Text(
              isConnected ? 'Requesting Signature' : '',
              style: TextStyle(
                color: isConnected ? theme.colorScheme.primary : null,
              ),
            );
          }
          return ListTile(
            enabled: isConnected,
            title: Text(label),
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
                foregroundColor: theme.colorScheme.error,
              ),
              child: Text('Cancel'),
            ),
          ),
        ),
      ],
    );
  }

  Widget buildBroadcastNeededColumn(BuildContext context) {
    final psbt = this.psbt;
    // Two ways there is nothing to send, and both hide the button rather than disable it: no
    // signing state at all — a screen on wallet history has no transaction of its own — or a
    // PSBT holding inputs that are not ours, whose signatures the template cannot carry, so
    // ours alone leave a transaction the network rejects.
    final params = widget.signingParams;
    final canBroadcast = params != null && params.unsignedTx.canBroadcast();
    // And one way there is nothing *yet*: signing still running. `signingDone` says only that
    // no stream is reporting progress, which is also true of a screen that never subscribed —
    // so ask whether the signatures exist rather than whether a stream is quiet.
    final haveSignatures = switch (params) {
      // True by construction: `SigningFinished.of` is the only way to build one and refuses
      // an empty list, so this does not have to re-check what the type already promises.
      SigningFinished() => true,
      TxNeedsSignatures() => signingState?.finishedSignatures != null,
      null => false,
    };

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
        if (canBroadcast)
          Flexible(
            child: FilledButton(
              onPressed: (haveSignatures && !isBroadcasting)
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
                  child: Text('Cancel'),
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
    final signingActive = widget.signingParams is TxNeedsSignatures;
    final signingColumn = Card.filled(
      margin: EdgeInsets.all(0.0),
      color: theme.colorScheme.surfaceContainerHigh,
      child: buildSignaturesNeededColumn(context),
    );
    return AnimatedCrossFade(
      firstChild: signingActive
          ? AnimatedGradientBorder(
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
              child: signingColumn,
            )
          : signingColumn,
      secondChild: Card.filled(
        color: Colors.transparent,
        margin: EdgeInsets.all(0.0),
        child: buildBroadcastNeededColumn(context),
      ),
      crossFadeState: signingDone
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
        content: Text(
          psbt != null
              ? 'Transaction canceled. No Bitcoin will be sent unless you have already exported the PSBT.'
              : 'No Bitcoin will be sent.',
        ),
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
        title: Text('Cancel Transaction'),
        content: Text('No Bitcoin will be sent.'),
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
    // The button is hidden in both these cases, so neither should be reachable — but a hidden
    // button is a fact about one screen, and this is the call that would put a transaction
    // the network rejects onto the network.
    // Where the signatures are depends on how this screen was opened, and there is no
    // sensible default for not having them — an empty list is not "sign with nothing", it is
    // "we cannot send this".
    final signingParams = widget.signingParams;
    final signatures = switch (signingParams) {
      SigningFinished(:final signatures) => signatures,
      TxNeedsSignatures() => signingState?.finishedSignatures,
      null => null,
    };
    if (signingParams == null ||
        signatures == null ||
        !signingParams.unsignedTx.canBroadcast()) {
      if (mounted) {
        setState(() => isBroadcasting = false);
        showErrorSnackbar(context, 'Nothing here can be broadcast');
      }
      return;
    }
    final RTransaction tx;
    try {
      tx = await signingParams.unsignedTx.withSignatures(
        signatures: signatures,
      );
    } catch (e) {
      // Nothing broadcastable was produced, so there is nothing safe to send.
      if (mounted) {
        setState(() => isBroadcasting = false);
        showErrorSnackbar(context, 'Cannot broadcast: $e');
      }
      return;
    }
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
        });
        // Remove signing session on successful broadcast.
        final finishedSsid = widget.finishedSigningSessionId;
        if (finishedSsid != null)
          await coord.forgetFinishedSignSession(ssid: finishedSsid);
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
            if (!txDetails.isConfirmed && signingDone)
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
  final feerate = txDetails.tx.feerate;
  return Column(
    children: [
      if (txDetails.isSend)
        ...txDetails.tx.recipients().where((info) => !info.isMine).map((info) {
          final address = info.address(network: walletCtx.network)?.toString();
          return Column(
            children: [
              CopyListTile(
                data: address,
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('Recipient #${info.vout}'),
                title: Text(
                  spacedHex(address ?? '<unknown>'),
                  style: monospaceTextStyle,
                  textAlign: TextAlign.end,
                ),
              ),
              CopyListTile(
                data: '${info.amount}',
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('\u2570 Amount'),
                title: SatoshiText(value: info.amount, showSign: false),
              ),
            ],
          );
        }),
      if (!txDetails.isSend)
        ...txDetails.tx.recipients().where((info) => info.isMine).map((info) {
          final address = info.address(network: walletCtx.network)?.toString();
          final idx = info.derivationIndex;
          return Column(
            children: [
              CopyListTile(
                data: address,
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('Received at${idx != null ? ' #$idx' : ''}'),
                title: Text(
                  spacedHex(address ?? '<unknown>'),
                  style: monospaceTextStyle,
                  textAlign: TextAlign.end,
                ),
              ),
              CopyListTile(
                data: '${info.amount}',
                dense: dense,
                contentPadding: contentPadding,
                leading: Text('\u2570 Amount'),
                title: SatoshiText(value: info.amount, showSign: false),
              ),
            ],
          );
        }),
      if (txDetails.isSend)
        CopyListTile(
          data: '$fee',
          dense: dense,
          contentPadding: contentPadding,
          leading: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('Fee '),
              // Omitted rather than shown empty: a transaction with an input we cannot
              // witness has no knowable signed size, and interpolating the null printed
              // "null sat/vB".
              if (feerate != null)
                Card.filled(
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6.0,
                      vertical: 2.0,
                    ),
                    child: Text(
                      '${feerate.toStringAsFixed(1)} sat/vB',
                      style: theme.textTheme.labelSmall,
                    ),
                  ),
                ),
            ],
          ),
          title: fee == null ? Text('Unknown') : SatoshiText(value: fee),
        ),
      if (showConfirmations)
        CopyListTile(
          data: '${txDetails.confirmations}',
          dense: dense,
          contentPadding: contentPadding,
          leading: Text('Confirmations'),
          title: Text(
            txDetails.isConfirmed
                ? '${txDetails.confirmations} Block(s)'
                : 'None',
            textAlign: TextAlign.end,
          ),
        ),
      CopyListTile(
        data: txDetails.tx.txid,
        dense: dense,
        contentPadding: contentPadding,
        leading: Text('Txid'),
        title: Text(
          txDetails.tx.txid,
          style: monospaceTextStyle,
          textAlign: TextAlign.end,
        ),
      ),
    ],
  );
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
