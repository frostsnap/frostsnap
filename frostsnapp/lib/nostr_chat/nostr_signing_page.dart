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
  final FfiNostrProfile? Function(PublicKey) getProfile;
  final PublicKey? myPubkey;
  final NostrClient client;
  final AccessStructureId accessStructureId;
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
    required this.accessStructureId,
    required this.nsec,
  });

  @override
  State<NostrSigningPage> createState() => _NostrSigningPageState();
}

class _NostrSigningPageState extends State<NostrSigningPage> {
  Set<DeviceId> connectedDevices = deviceIdSet([]);
  StreamSubscription<DeviceListUpdate>? devicesSub;
  StreamSubscription<SigningState>? signingSub;
  SigningState? localSigningState;
  late final FullscreenActionDialogController actionDialogController;

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
      asRef: widget.signingState.request.accessStructureRef,
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

    actionDialogController = FullscreenActionDialogController(
      title: 'Sign transaction with connected device(s)',
      actionButtons: [
        Builder(
          builder: (context) => OutlinedButton(
            child: Text('Cancel'),
            onPressed: () => Navigator.pop(context),
          ),
        ),
        DeviceActionHint(),
      ],
      onDismissed: () {},
    );

    devicesSub = GlobalStreams.deviceListSubject.listen(_onDeviceListData);
    widget.signingState.addListener(_onSigningRequestChanged);

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _iHaveSigned) return;
      final devices = _myDevices;
      if (devices.isNotEmpty) {
        actionDialogController.batchAddActionNeeded(context, devices);
      }
    });

    _tryStartSigningSession();
  }

  Future<void> _tryStartSigningSession() async {
    if (!_iHaveOffered || _iHaveSigned || signingSub != null) return;

    final sealed = widget.signingState.sealedData;
    if (sealed == null) return;

    final devices = _myDevices;
    if (devices.isEmpty) return;

    final reservationId = NonceReservationId(
      field0: widget.signingState.request.eventId.field0,
    );

    SignSessionId? sessionId;
    try {
      for (final deviceId in devices) {
        sessionId = await coord.signWithNonceReservation(
          signTask: sealed.signTask(),
          accessStructureRef: sealed.accessStructureRef(),
          allBinonces: sealed.binonces(),
          id: reservationId,
          deviceId: deviceId,
        );
      }
    } catch (_) {
      return;
    }

    if (sessionId == null || !mounted) return;

    late final StreamSubscription<SigningState> sub;
    sub = coord.tryRestoreSigningSession(sessionId: sessionId).listen((state) {
      sub.pause();
      _onSigningData(state).whenComplete(sub.resume);
    });
    signingSub = sub;
  }

  void _onSigningRequestChanged() {
    if (mounted) {
      setState(() {});
      _tryStartSigningSession();
    }
  }

  @override
  void dispose() {
    widget.signingState.removeListener(_onSigningRequestChanged);
    devicesSub?.cancel();
    if (signingSub?.cancel() != null) {
      coord.cancelProtocol();
    }
    actionDialogController.dispose();
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

    // Send partial to nostr if our device just signed
    if (!_iHaveSigned) {
      for (final deviceId in _myDevices) {
        if (data.gotShares.any((id) => deviceIdEquals(id, deviceId))) {
          final shares = coord.getDeviceSignatureShares(
            sessionId: data.sessionId,
            deviceId: deviceId,
          );
          if (shares != null) {
            await widget.client.sendSignPartial(
              accessStructureId: widget.accessStructureId,
              nsec: widget.nsec,
              requestId: widget.signingState.request.eventId,
              sessionId: data.sessionId,
              shares: shares,
            );
          }
        }
      }
    }

    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    for (final id in data.connectedButNeedRequest) {
      coord.requestDeviceSign(
        deviceId: id,
        sessionId: data.sessionId,
        encryptionKey: encryptionKey,
      );
    }
    await actionDialogController.batchRemoveActionNeeded(data.gotShares);
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
      asRef: widget.signingState.request.accessStructureRef,
    );

    return [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
        child: Text(
          _iHaveSigned
              ? 'Signed!'
              : devices.length == 1
                  ? 'Plug in ${coord.getDeviceName(id: devices.first) ?? "device"} to sign'
                  : 'Plug in your devices to sign',
          style: theme.textTheme.titleSmall,
          textAlign: TextAlign.center,
        ),
      ),
      ...devices.map((deviceId) {
        final deviceName = coord.getDeviceName(id: deviceId) ?? '<no-name>';
        final shareIndex = accessStruct?.getDeviceShortShareIndex(deviceId: deviceId);
        final label = shareIndex != null ? '#$shareIndex $deviceName' : deviceName;
        final isConnected = connectedDevices.contains(deviceId);
        final hasSigned = _iHaveSigned || (localSigningState?.gotShares.any(
          (id) => deviceIdEquals(deviceId, id),
        ) ?? false);
        return ListTile(
          dense: true,
          enabled: isConnected,
          leading: Icon(Icons.key, size: 20),
          title: Text(label),
          trailing: hasSigned
              ? AnimatedCheckCircle()
              : Text(
                  isConnected ? 'Requesting Signature' : '',
                  style: TextStyle(color: isConnected ? theme.colorScheme.primary : null),
                ),
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
                      style: TextButton.styleFrom(
                        foregroundColor: theme.colorScheme.error,
                      ),
                      child: Text('Cancel'),
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
  final FfiNostrProfile? profile;
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
