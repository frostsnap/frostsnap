import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/signing_card.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:glowy_borders/glowy_borders.dart';

class NostrSigningPage extends StatefulWidget {
  final ScrollController? scrollController;
  final TxDetailsModel txDetails;
  final SigningRequestState signingState;
  final int threshold;
  final NostrProfile? Function(PublicKey) getProfile;
  final PublicKey? myPubkey;
  final NostrClient client;
  final AccessStructureRef accessStructureRef;
  final String nsec;

  const NostrSigningPage({
    super.key,
    this.scrollController,
    required this.txDetails,
    required this.signingState,
    required this.threshold,
    required this.getProfile,
    this.myPubkey,
    required this.client,
    required this.accessStructureRef,
    required this.nsec,
  });

  AccessStructureId get accessStructureId => accessStructureRef.accessStructureId;

  @override
  State<NostrSigningPage> createState() => _NostrSigningPageState();
}

class _NostrSigningPageState extends State<NostrSigningPage> {
  Set<DeviceId> connectedDevices = deviceIdSet([]);
  StreamSubscription<DeviceListUpdate>? devicesSub;
  StreamSubscription<SigningState>? signingSub;
  SigningSessionHandle? _handle;
  SigningState? localSigningState;
  FullscreenActionDialogController<void>? actionDialogController;
  bool _cancelRequested = false;
  /// Set when the remote sign session's stream closes normally — i.e. all
  /// our local targets have contributed and the dispatcher was popped off
  /// the ui_stack. Used by `dispose` to skip `_handle.cancel()` when the
  /// session finished cleanly.
  bool _remoteSessionComplete = false;
  /// Periodic re-publish of any shares that core has cached but nostr hasn't
  /// echoed back. Covers app-restart-mid-publish and transient relay failures.
  Timer? _broadcastRetryTimer;
  /// Single-flight guard on `_reconcileUnbroadcastShares` so the periodic
  /// timer and listener-driven triggers don't race.
  bool _reconciling = false;

  static const margin = EdgeInsets.only(left: 16.0, right: 16.0, bottom: 16.0);

  bool get _iHaveSigned {
    final myHex = widget.myPubkey?.toHex();
    return myHex != null && widget.signingState.partials.containsKey(myHex);
  }

  bool get _iHaveOffered {
    final myHex = widget.myPubkey?.toHex();
    return myHex != null && widget.signingState.offers.containsKey(myHex);
  }

  List<DeviceId> get _myDevices {
    final myHex = widget.myPubkey?.toHex();
    if (myHex == null) return [];
    final myOffer = widget.signingState.offers[myHex];
    if (myOffer == null) return [];
    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    if (accessStruct == null) return [];
    return myOffer.shareIndices.map((idx) {
      for (final deviceId in accessStruct.devices()) {
        if (accessStruct.getDeviceShortShareIndex(deviceId: deviceId) == idx) {
          return deviceId;
        }
      }
      return null;
    }).whereType<DeviceId>().toList();
  }

  @override
  void initState() {
    super.initState();

    devicesSub = GlobalStreams.deviceListSubject.listen(_onDeviceListData);
    widget.signingState.addListener(_onSigningRequestChanged);

    WidgetsBinding.instance.addPostFrameCallback((_) {
      _ensureDialogController();
    });

    _tryStartSigningSession();
    _reconcileUnbroadcastShares();
    _broadcastRetryTimer = Timer.periodic(
      const Duration(seconds: 15),
      (_) => _reconcileUnbroadcastShares(),
    );
  }

  /// For each local device whose signature is already cached in core, make
  /// sure the corresponding nostr `Partial` has been (re-)sent. Source of
  /// truth for "already on nostr" is `widget.signingState.partials` — that
  /// only updates when we observe the echo from a relay, so if our send
  /// was dropped it stays false and we retry.
  Future<void> _reconcileUnbroadcastShares() async {
    if (!mounted || _iHaveSigned || _reconciling) return;

    final offerSubset = widget.signingState.sealedOfferSubset;
    if (offerSubset == null) return;

    final reservationId = RemoteSignSessionId(
      field0: widget.signingState.request.eventId.field0,
    );
    final completed = coord.getCompletedSignatureShares(id: reservationId);
    if (completed.isEmpty) return;

    _reconciling = true;
    try {
      for (final entry in completed) {
        if (!mounted || _iHaveSigned) break;
        try {
          await widget.client.sendSignPartial(
            accessStructureId: widget.accessStructureId,
            nsec: widget.nsec,
            requestId: widget.signingState.request.eventId,
            offerSubset: offerSubset,
            shares: entry.$2,
          );
        } catch (e) {
          debugPrint('[nostr-signing] reconcile sendSignPartial failed: $e');
        }
      }
    } finally {
      _reconciling = false;
    }
  }

  void _ensureDialogController() {
    if (!mounted || _iHaveSigned || actionDialogController != null) return;
    final devices = _myDevices;
    if (devices.isEmpty) return;

    actionDialogController = FullscreenActionDialogController<void>(
      context: context,
      devices: devices,
      title: 'Sign transaction with connected device(s)',
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () {
            // Cancel closes the entire signing flow, not just the fullscreen
            // dialog. Disable the controller (layering rule: never pop the
            // dialog from outside) and defer popping the parent sheet to
            // onDismissed, which runs once the fullscreen dialog has fully
            // torn down.
            _cancelRequested = true;
            actionDialogController?.enabled = false;
          },
        ),
        DeviceActionHint(),
      ],
      onDismissed: () {
        if (!_cancelRequested || !mounted) return;
        _cancelRequested = false;
        // Pop the signing sheet/dialog that hosts this page. That route's
        // dispose will take down the NostrSigningPage state, which disposes
        // the controller.
        Navigator.of(context).pop();
      },
    );
  }

  Future<void> _tryStartSigningSession() async {
    if (!_iHaveOffered || _iHaveSigned || signingSub != null) return;

    final sealed = widget.signingState.sealedData;
    if (sealed == null) return;

    final devices = _myDevices;
    if (devices.isEmpty) return;

    final reservationId = RemoteSignSessionId(
      field0: widget.signingState.request.eventId.field0,
    );

    // Seed from core: which of our local devices have already returned a
    // share (e.g. after resume). If every device is done there's nothing
    // left for the dispatcher to watch, so don't sub.
    final completedIds = coord
        .getCompletedSignatureShares(id: reservationId)
        .map((entry) => entry.$1)
        .toList();
    final hasOutstanding = devices.any(
      (d) => !completedIds.any((c) => deviceIdEquals(c, d)),
    );
    if (!hasOutstanding) return;

    // Install the remote signing dispatcher and keep the handle. The handle
    // is how we push `signWithNonceReservation` / cancel commands into the
    // dispatcher; the stream from `subState()` is how the dispatcher talks
    // back. `signWithNonceReservation` is only called when the dispatcher
    // tells us a device has connected and still needs a request (see
    // `connectedButNeedRequest` in `_onSigningData`).
    final SigningSessionHandle handle;
    try {
      handle = await coord.subRemoteSignSession(
        remoteSignSessionId: reservationId,
        allBinonces: sealed.binonces(),
        targets: devices,
        gotSignatures: completedIds,
      );
    } catch (e) {
      debugPrint('[nostr-signing] subRemoteSignSession failed: $e');
      return;
    }
    if (!mounted) {
      await handle.cancel();
      return;
    }
    _handle = handle;

    late final StreamSubscription<SigningState> sub;
    sub = handle.subState().start().listen(
      (state) {
        sub.pause();
        _onSigningData(state).whenComplete(sub.resume);
      },
      onError: (error) {
        debugPrint('[nostr-signing] subState error: $error');
      },
      onDone: () {
        // The Rust dispatcher completes once every target device has
        // returned a share, at which point the stream closes cleanly.
        // Flag it so dispose doesn't treat this like an abort.
        _remoteSessionComplete = true;
        signingSub = null;
      },
    );
    signingSub = sub;
  }

  void _onSigningRequestChanged() {
    if (mounted) {
      setState(() {});
      _tryStartSigningSession();
      _reconcileUnbroadcastShares();
      _ensureDialogController();
    }
  }

  @override
  void dispose() {
    widget.signingState.removeListener(_onSigningRequestChanged);
    devicesSub?.cancel();
    _broadcastRetryTimer?.cancel();
    _broadcastRetryTimer = null;
    signingSub?.cancel();
    signingSub = null;
    final handle = _handle;
    _handle = null;
    if (handle != null && !_remoteSessionComplete) {
      // Cancel this specific session only. Unlike the old blunt
      // `coord.cancelProtocol()` (which is `cancel_all` on the stack),
      // this leaves any concurrent backup / keygen / other signing
      // dispatchers alone.
      handle.cancel();
    }
    actionDialogController?.dispose();
    super.dispose();
  }

  void _onDeviceListData(DeviceListUpdate data) {
    if (mounted) {
      setState(() {
        connectedDevices.clear();
        connectedDevices.addAll(data.state.devices.map((dev) => dev.id));
      });
    }
  }

  Future<void> _onSigningData(SigningState data) async {
    if (!mounted) return;

    setState(() => localSigningState = data);

    // Drive signing for newly-connected target devices. The handle owns the
    // remote context (sign_task, binonces, reservation id); the per-device
    // call only needs device_id + encryption_key, same shape as local.
    final handle = _handle;
    if (handle != null) {
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      for (final deviceId in data.connectedButNeedRequest) {
        try {
          await handle.requestDeviceSign(
            deviceId: deviceId,
            encryptionKey: encryptionKey,
          );
        } catch (e) {
          debugPrint('[nostr-signing] requestDeviceSign error: $e');
        }
      }
    }
    await actionDialogController?.batchRemoveActionNeeded(data.gotShares);

    // Any device that just signed now has cached shares in core. Fire the
    // reconciler to push it out to nostr; the `_iHaveSigned` guard + the
    // single-flight `_reconciling` flag handle dedup and retry.
    await _reconcileUnbroadcastShares();
  }

  Widget _buildSignersSection(BuildContext context, ThemeData theme) {
    final state = widget.signingState;
    final totalSigned = state.partials.length;
    final threshold = widget.threshold;
    final myHex = widget.myPubkey?.toHex();
    final myOffer = myHex != null ? state.offers[myHex] : null;
    final otherOffers = state.offers.entries.where((e) => e.key != myHex).toList();

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Signers',
                  style: theme.textTheme.labelMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
              Stack(
                alignment: AlignmentDirectional.center,
                children: [
                  SizedBox(
                    width: 40, height: 40,
                    child: CircularProgressIndicator(
                      value: totalSigned / (threshold > 0 ? threshold : 1),
                      backgroundColor: theme.colorScheme.surfaceContainerHighest,
                      strokeCap: StrokeCap.round,
                      strokeWidth: 3,
                    ),
                  ),
                  Text('$totalSigned/$threshold', style: theme.textTheme.labelSmall),
                ],
              ),
            ],
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 16,
            runSpacing: 8,
            alignment: WrapAlignment.center,
            children: [
              if (myOffer != null)
                _SignerChip(
                  profile: widget.getProfile(myOffer.author),
                  pubkey: myOffer.author,
                  name: 'You',
                  keyLabel: myOffer.shareIndices.map((i) => '#$i').join(', '),
                  signed: state.partials.containsKey(myHex),
                ),
              ...otherOffers.map((entry) {
                final offer = entry.value;
                final hasSigned = state.partials.containsKey(entry.key);
                final profile = widget.getProfile(offer.author);
                return _SignerChip(
                  profile: profile,
                  pubkey: offer.author,
                  name: getDisplayName(profile, offer.author),
                  keyLabel: offer.shareIndices.map((i) => '#$i').join(', '),
                  signed: hasSigned,
                );
              }),
            ],
          ),
        ],
      ),
    );
  }

  List<Widget> _buildDeviceContent(BuildContext context, ThemeData theme) {
    final devices = _myDevices;
    if (devices.isEmpty) return [];

    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );

    final reservationId = RemoteSignSessionId(
      field0: widget.signingState.request.eventId.field0,
    );
    final coreSignedIds = coord
        .getCompletedSignatureShares(id: reservationId)
        .map((entry) => entry.$1)
        .toList();
    bool coreSigned(DeviceId id) =>
        coreSignedIds.any((d) => deviceIdEquals(d, id));
    // `_iHaveSigned` is keyed by our author pubkey — it flips true the moment
    // nostr has observed any partial from us. In the single-local-device
    // case that matches "our share is on the wire"; for multi-device it's
    // a pre-existing coarsening of state we're not fixing here.
    final nostrConfirmed = _iHaveSigned;
    final anyCoreSigned = devices.any(coreSigned);
    final broadcastingAny = anyCoreSigned && !nostrConfirmed;

    final headerText = nostrConfirmed
        ? 'Signed!'
        : broadcastingAny
            ? 'Broadcasting signatures…'
            : devices.length == 1
                ? 'Plug in ${coord.getDeviceName(id: devices.first) ?? "device"} to sign'
                : 'Plug in your devices to sign';

    return [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
        child: Text(
          headerText,
          style: theme.textTheme.titleSmall,
          textAlign: TextAlign.center,
        ),
      ),
      ...devices.map((deviceId) {
        final deviceName = coord.getDeviceName(id: deviceId) ?? '<no-name>';
        final shareIndex = accessStruct?.getDeviceShortShareIndex(deviceId: deviceId);
        final label = shareIndex != null ? '#$shareIndex $deviceName' : deviceName;
        final isConnected = connectedDevices.contains(deviceId);
        final deviceCoreSigned = coreSigned(deviceId);
        final Widget trailing;
        if (nostrConfirmed) {
          trailing = AnimatedCheckCircle();
        } else if (deviceCoreSigned) {
          trailing = Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: 14,
                height: 14,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              const SizedBox(width: 8),
              Text(
                'Broadcasting',
                style: TextStyle(color: theme.colorScheme.onSurfaceVariant),
              ),
            ],
          );
        } else {
          trailing = Text(
            isConnected ? 'Requesting Signature' : '',
            style: TextStyle(
              color: isConnected ? theme.colorScheme.primary : null,
            ),
          );
        }
        return ListTile(
          dense: true,
          enabled: isConnected || deviceCoreSigned || nostrConfirmed,
          leading: Icon(Icons.key, size: 20),
          title: Text(label),
          trailing: trailing,
        );
      }),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final needsAction = _iHaveOffered && !_iHaveSigned;

    final deviceContent = _buildDeviceContent(context, theme);

    final deviceCard = deviceContent.isNotEmpty
        ? Card.filled(
            margin: EdgeInsets.zero,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(12.0),
            ),
            color: theme.colorScheme.surfaceContainerHigh,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                ...deviceContent,
                Divider(height: 0.0),
                Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: Padding(
                    padding: EdgeInsets.symmetric(vertical: 4.0, horizontal: 12.0),
                    child: TextButton(
                      onPressed: () => Navigator.pop(context),
                      child: Text(_iHaveSigned ? 'Done' : 'Cancel'),
                    ),
                  ),
                ),
              ],
            ),
          )
        : null;

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
                    txDetails: widget.txDetails,
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
                    txDetails: widget.txDetails,
                    dense: true,
                    showConfirmations: false,
                  ),
                ),
              ),
              _buildSignersSection(context, theme),
              if (deviceCard != null) ...[
                const SizedBox(height: 8),
                AnimatedGradientBorder(
                  stretchAlongAxis: true,
                  borderSize: 1.0,
                  glowSize: 5.0,
                  animationTime: 6,
                  borderRadius: BorderRadius.circular(12.0),
                  gradientColors: needsAction
                      ? [
                          theme.colorScheme.outlineVariant,
                          theme.colorScheme.primary,
                          theme.colorScheme.secondary,
                          theme.colorScheme.tertiary,
                        ]
                      : [Colors.transparent, Colors.transparent],
                  child: deviceCard,
                ),
              ],
            ]),
          ),
        ),
      ],
    );
  }
}

class _SignerChip extends StatelessWidget {
  final NostrProfile? profile;
  final PublicKey pubkey;
  final String name;
  final String keyLabel;
  final bool signed;

  const _SignerChip({
    required this.profile,
    required this.pubkey,
    required this.name,
    required this.keyLabel,
    required this.signed,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Stack(
          children: [
            NostrAvatar.small(profile: profile, pubkey: pubkey),
            if (signed)
              Positioned(
                right: -2, bottom: -2,
                child: Container(
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surface,
                    shape: BoxShape.circle,
                  ),
                  padding: const EdgeInsets.all(1),
                  child: Icon(Icons.check_circle, size: 16, color: Colors.green),
                ),
              ),
            if (!signed)
              Positioned(
                right: -2, bottom: -2,
                child: Container(
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surface,
                    shape: BoxShape.circle,
                  ),
                  padding: const EdgeInsets.all(1),
                  child: Icon(Icons.hourglass_empty, size: 14, color: theme.colorScheme.outline),
                ),
              ),
          ],
        ),
        const SizedBox(height: 4),
        Text.rich(
          TextSpan(children: [
            TextSpan(text: '$keyLabel ', style: theme.textTheme.labelSmall),
            TextSpan(
              text: name,
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ]),
          overflow: TextOverflow.ellipsis,
        ),
      ],
    );
  }
}
