import 'dart:async';
import 'dart:collection';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:rxdart/rxdart.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';

import 'global.dart';

class AddressList extends StatefulWidget {
  final ScrollController? scrollController;
  final bool showUsed;
  final Function(BuildContext, AddressInfo) onTap;
  final int? scrollToDerivationIndex;

  const AddressList({
    super.key,
    required this.onTap,
    this.showUsed = false,
    this.scrollToDerivationIndex,
    this.scrollController,
  });

  @override
  State<AddressList> createState() => _AddressListState();
}

class _AddressListState extends State<AddressList> {
  List<AddressInfo> _addresses = [];
  List<AddressInfo> get addresses => _addresses;

  final _firstAddrKey = GlobalKey();
  late final ScrollController? _scrollController;
  ScrollController get scrollController =>
      widget.scrollController ?? _scrollController!;

  void update(BuildContext context, {void Function()? andSetState}) async {
    final walletCtx = WalletContext.of(context);
    if (walletCtx != null) {
      if (mounted) {
        setState(() {
          _addresses = walletCtx.wallet.addressesState();
          if (andSetState != null) andSetState();
        });
      }
    }
  }

  @override
  void initState() {
    super.initState();
    _scrollController = widget.scrollController == null
        ? ScrollController()
        : null;

    update(context);

    // Scroll to the given derivation index (if requested).
    final startIndex = widget.scrollToDerivationIndex;
    if (startIndex != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        late final double addrItemHeight;
        final addrItemCtx = _firstAddrKey.currentContext;
        if (addrItemCtx != null) {
          final render = addrItemCtx.findRenderObject() as RenderBox;
          addrItemHeight = render.size.height;
        } else {
          addrItemHeight = 0;
        }
        final targetIndex =
            addresses.indexed
                .firstWhereOrNull((ia) => ia.$2.index == startIndex)
                ?.$1 ??
            0;
        final targetOffset =
            scrollController.offset + targetIndex * addrItemHeight;
        scrollController.animateTo(
          targetOffset,
          duration: Durations.long4,
          curve: Curves.easeInOutCubicEmphasized,
        );
      });
    }
  }

  @override
  dispose() {
    if (_scrollController != null) _scrollController.dispose();
    super.dispose();
  }

  Widget buildAddressItem(BuildContext context, AddressInfo addr, {Key? key}) {
    final theme = Theme.of(context);
    final usedTag = Card.outlined(
      color: Colors.transparent,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
        child: Text('Used', style: TextStyle(color: theme.colorScheme.error)),
      ),
    );

    return ListTile(
      key: key,
      onTap: () {
        Navigator.pop(context);
        widget.onTap(context, addr);
      },
      tileColor:
          widget.scrollToDerivationIndex != null &&
              widget.scrollToDerivationIndex == addr.index
          ? theme.colorScheme.surfaceContainerHighest
          : null,
      leading: Text(
        '#${addr.index}',
        style: theme.textTheme.labelLarge?.copyWith(
          decoration: addr.used ? TextDecoration.lineThrough : null,
          color: theme.colorScheme.primary,
          decorationThickness: addr.used ? 3.0 : null,
          fontFamily: monospaceTextStyle.fontFamily,
        ),
      ),
      title: Text(
        spacedHex(addr.address.toString()),
        style: monospaceTextStyle.copyWith(
          color: addr.used ? theme.colorScheme.onSurfaceVariant : null,
        ),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 4,
        children: [if (addr.used) usedTag, Icon(Icons.chevron_right_rounded)],
      ),
      contentPadding: EdgeInsets.symmetric(horizontal: 20),
    );
  }

  @override
  Widget build(BuildContext context) {
    var first = true;
    return CustomScrollView(
      controller: scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        SliverSafeArea(
          sliver: SliverPadding(
            padding: const EdgeInsets.symmetric(vertical: 8.0),
            sliver: SliverList.list(
              children: addresses
                  .map(
                    (addr) => buildAddressItem(
                      context,
                      addr,
                      key: () {
                        if (first) {
                          first = false;
                          return _firstAddrKey;
                        } else {
                          return null;
                        }
                      }(),
                    ),
                  )
                  .toList(),
            ),
          ),
        ),
      ],
    );
  }
}

enum ReceivePageFocus { share, verify, awaitTx }

class ReceivePage extends StatefulWidget {
  final ScrollController? scrollController;
  final Wallet wallet;
  final Stream<TxState> txStream;
  final int? derivationIndex;

  const ReceivePage({
    super.key,
    this.scrollController,
    required this.wallet,
    required this.txStream,
    this.derivationIndex,
  });

  @override
  State<ReceivePage> createState() => _ReceiverPageState();
}

class _ReceiverPageState extends State<ReceivePage> {
  late final StreamSubscription txStreamSub;
  List<Transaction> allTxs = [];

  BehaviorSubject<VerifyAddressProtocolState>? _verifyStream;
  late final FullscreenActionDialogController fullscreenDialogController;
  bool verificationSuccess = false;

  ReceivePageFocus _focus = ReceivePageFocus.share;
  ReceivePageFocus get focus => _focus;
  set focus(ReceivePageFocus v) {
    if (v == _focus || _address == null) return;
    if (v == ReceivePageFocus.verify) {
      final stream = coord
          .verifyAddress(
            keyId: widget.wallet.keyId(),
            addressIndex: _address!.index,
          )
          .toBehaviorSubject();
      setState(() {
        _verifyStream = stream;
        _focus = v;
      });
      return;
    }
    if (_verifyStream != null) coord.cancelProtocol();
    setState(() {
      _verifyStream = null;
      _focus = v;
    });
  }

  bool get isRevealed => _address?.revealed ?? false;
  bool get isUsed => _address?.used ?? false;

  AddressInfo? _address;
  bool get isReady => _address != null;
  Wallet get wallet => widget.wallet;

  static const tilePadding = EdgeInsets.symmetric(horizontal: 20);
  static const tileShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(12)),
  );
  static const sectionPadding = EdgeInsets.fromLTRB(20, 0, 20, 20);
  static const sectionHideDuration = Durations.medium4;
  static const sectionHideCurve = Curves.easeInOutCubicEmphasized;

  QrImage addressQrImage(AddressInfo address) {
    final qrCode = QrCode(8, QrErrorCorrectLevel.L);
    // we don't use any other BIP21 params yet
    qrCode.addData('bitcoin:${address.address}');
    return QrImage(qrCode);
  }

  @override
  void initState() {
    super.initState();

    fullscreenDialogController = FullscreenActionDialogController(
      title: 'Verify address on device',
      body: _dialogBodyBuilder,
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () {
            verificationSuccess = false;
            Navigator.pop(context);
          },
        ),
        OutlinedButton(
          onPressed: () {
            verificationSuccess = true;
            Navigator.pop(context);
          },
          child: Text('Sender has correct address'),
        ),
      ],
      onDismissed: () {
        if (verificationSuccess) {
          focus = ReceivePageFocus.awaitTx;
        } else {
          focus = ReceivePageFocus.share;
        }
        verificationSuccess = false;
      },
    );

    final startIndex = widget.derivationIndex ?? wallet.nextAddress().index;
    updateToIndex(startIndex);

    txStreamSub = widget.txStream.listen((txState) {
      if (context.mounted) {
        AddressInfo? addr;
        final index = _address?.index;
        if (index != null) {
          addr = wallet.getAddressInfo(index);
        }
        setState(() {
          allTxs = txState.txs;
          if (addr != null) _address = addr;
        });
      }
    });
  }

  @override
  void dispose() {
    if (_focus == ReceivePageFocus.verify) {
      coord.cancelProtocol();
    }
    txStreamSub.cancel();
    fullscreenDialogController.dispose();
    super.dispose();
  }

  void updateToIndex(int index, {ReceivePageFocus? next}) {
    final addr = wallet.getAddressInfo(index);
    if (mounted) {
      setState(() {
        _address = addr;
        if (next != null) _focus = next;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        SliverToBoxAdapter(child: buildShareCard(context)),
        SliverToBoxAdapter(child: buildVerifyCard(context)),
        SliverToBoxAdapter(child: activityCard(context)),
        SliverSafeArea(
          sliver: SliverPadding(padding: EdgeInsets.only(bottom: 4)),
        ),
      ],
    );
  }

  Widget buildShareCard(BuildContext context) {
    final isFocused = focus == ReceivePageFocus.share;
    final theme = Theme.of(context);

    final header = ListTile(
      shape: tileShape,
      contentPadding: tilePadding.copyWith(right: 8),
      title: Text('Share'),
      trailing: TextButton.icon(
        onPressed: _address == null
            ? null
            : () => openAddressPicker(context, _address!),
        label: Text(
          '#${_address?.index}',
          style: monospaceTextStyle.copyWith(
            decoration: isUsed ? TextDecoration.lineThrough : null,
            decorationThickness: isUsed ? 3.0 : null,
          ),
        ),
        icon: Icon(Icons.arrow_drop_down_rounded),
      ),
      onTap: switch (focus) {
        ReceivePageFocus.share => null,
        _ => () => focus = ReceivePageFocus.share,
      },
    );
    final addressText = Text(
      spacedHex(_address?.address.toString() ?? ''),
      style: theme.textTheme.labelLarge?.copyWith(
        fontFamily: monospaceTextStyle.fontFamily,
        color: theme.colorScheme.onSurfaceVariant,
      ),
      textAlign: TextAlign.start,
    );

    final cardBody = Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Padding(
          padding: tilePadding.copyWith(top: 20, bottom: 20),
          child: addressText,
        ),
        Padding(
          padding: EdgeInsets.symmetric(vertical: 12, horizontal: 20),
          child: Row(
            spacing: 12,
            children: [
              Expanded(
                child: FilledButton.tonalIcon(
                  onPressed: _address == null
                      ? null
                      : () => copyAddress(context, _address!),
                  label: Text('Copy'),
                  icon: Icon(Icons.copy_rounded),
                ),
              ),
              Expanded(
                child: FilledButton.tonalIcon(
                  onPressed: _address == null
                      ? null
                      : () async => showAddressQr(context, _address!),
                  label: Text('QR Code'),
                  icon: Icon(Icons.qr_code_2_rounded),
                ),
              ),
            ],
          ),
        ),
      ],
    );
    final activeCard = Card.outlined(
      margin: sectionPadding,
      color: theme.colorScheme.surfaceContainerHigh,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [header, cardBody],
      ),
    );
    final inactiveCard = Card.outlined(
      // We cannot control the size of the `AnimatedGradientBorder` of the verify card.
      margin: focus == ReceivePageFocus.verify ? tilePadding : sectionPadding,
      color: theme.colorScheme.surfaceContainerLow,
      child: Column(mainAxisSize: MainAxisSize.min, children: [header]),
    );
    return AnimatedCrossFade(
      firstChild: activeCard,
      secondChild: inactiveCard,
      crossFadeState: isFocused
          ? CrossFadeState.showFirst
          : CrossFadeState.showSecond,
      duration: sectionHideDuration,
      sizeCurve: sectionHideCurve,
    );
  }

  Widget buildVerifyCard(BuildContext context) {
    final isFocused = focus == ReceivePageFocus.verify;

    final theme = Theme.of(context);
    final activeCard = AnimatedGradientBorder(
      borderSize: 1,
      glowSize: 6,
      animationTime: 6,
      borderRadius: BorderRadius.circular(12.0),
      gradientColors: [
        theme.colorScheme.outlineVariant,
        theme.colorScheme.primary,
        theme.colorScheme.secondary,
        theme.colorScheme.tertiary,
      ],
      child: Card.filled(
        margin: EdgeInsets.zero,
        color: theme.colorScheme.surfaceContainerHigh,
        child: _verifyStream == null
            ? null
            : StreamBuilder(
                stream: _verifyStream,
                builder: (context, targetDevicesSnapshot) {
                  final targetDevices = deviceIdSet(
                    targetDevicesSnapshot.data?.targetDevices ?? [],
                  );

                  final dismissButton = OutlinedButton(
                    onPressed: () => focus = ReceivePageFocus.awaitTx,
                    child: Text('Skip'),
                  );

                  return StreamBuilder(
                    stream: GlobalStreams.deviceListSubject,
                    builder: (context, deviceListSnapshot) {
                      final connectedDevices = deviceIdSet(
                        deviceListSnapshot.data?.state.devices
                                .map((dev) => dev.id)
                                .toList() ??
                            [],
                      );

                      final displayingDevices = targetDevices.intersection(
                        connectedDevices,
                      );

                      fullscreenDialogController.batchAddActionNeeded(
                        context,
                        displayingDevices,
                      );

                      // collect the list first because we're going to mutate it
                      fullscreenDialogController.clearAllExcept(
                        displayingDevices,
                      );

                      return Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          ListTile(
                            shape: tileShape,
                            title: Text('Verify'),
                            contentPadding: tilePadding,
                            minVerticalPadding: 16,
                          ),
                          Padding(
                            padding: sectionPadding.copyWith(
                              top: 20,
                              bottom: 36,
                            ),
                            child: Text(
                              'Plug in a device to verify the address on a device screen.',
                              softWrap: true,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                          Padding(
                            padding: sectionPadding,
                            child: dismissButton,
                          ),
                        ],
                      );
                    },
                  );
                },
              ),
      ),
    );
    final inactiveCard = Card.outlined(
      margin: sectionPadding,
      color: theme.colorScheme.surfaceContainerLow,
      child: ListTile(
        shape: tileShape,
        title: Text('Verify'),
        contentPadding: tilePadding,
        onTap: () => focus = ReceivePageFocus.verify,
      ),
    );
    return AnimatedSize(
      clipBehavior: Clip.none,
      alignment: AlignmentDirectional.topCenter,
      duration: sectionHideDuration,
      curve: sectionHideCurve,
      child: isFocused ? activeCard : inactiveCard,
    );
  }

  Widget activityCard(BuildContext context) {
    final fsCtx = FrostsnapContext.of(context)!;
    final thisAddr = _address?.address;
    final thisSpk = _address?.address.spk();
    final walletCtx = WalletContext.of(context)!;
    final isFocused = focus == ReceivePageFocus.awaitTx;
    final theme = Theme.of(context);
    final now = DateTime.now();
    final chainTipHeight = walletCtx.wallet.superWallet.height();

    final relevantTxs = allTxs.where((tx) {
      if (thisAddr == null || thisSpk == null) return false;
      final netSpent = (tx.sumInputsSpendingSpk(spk: thisSpk) ?? 0);
      final netReceived = tx.sumOutputsToSpk(spk: thisSpk);
      return (netSpent > 0 || netReceived > 0);
    }).toList();
    final txTiles = relevantTxs.map((tx) {
      final txDetails = TxDetailsModel(
        tx: tx,
        chainTipHeight: chainTipHeight,
        now: now,
      );
      return TxSentOrReceivedTile(
        txDetails: txDetails,
        onTap: () => showBottomSheetOrDialog(
          context,
          title: Text('Transaction Details'),
          builder: (context, scrollController) => walletCtx.wrap(
            TxDetailsPage(
              scrollController: scrollController,
              txStates: walletCtx.txStream,
              txDetails: txDetails,
              psbtMan: fsCtx.psbtManager,
            ),
          ),
        ),
      );
    });
    final subtitle = Text(switch (relevantTxs.length) {
      1 => '1 transaction',
      _ => '${relevantTxs.length} transactions',
    });
    final header = ListTile(
      shape: tileShape,
      contentPadding: tilePadding,
      title: Text('Activity'),
      trailing: subtitle,
      onTap: isFocused ? null : () => focus = ReceivePageFocus.awaitTx,
    );
    final activeCard = Card.outlined(
      margin: sectionPadding,
      color: theme.colorScheme.surfaceContainerHigh,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          header,
          Column(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.start,
            children: [
              ...txTiles,
              if (relevantTxs.isEmpty)
                Padding(
                  padding: EdgeInsets.fromLTRB(24, 0, 24, 24),
                  child: AspectRatio(
                    aspectRatio: 4,
                    child: Center(
                      child: Text(
                        'Waiting for transactions...',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: theme.colorScheme.outline),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
    final inactiveCard = Card.outlined(
      margin: isFocused ? tilePadding : sectionPadding,
      color: theme.colorScheme.surfaceContainerLow,
      child: Column(mainAxisSize: MainAxisSize.min, children: [header]),
    );
    return AnimatedCrossFade(
      firstChild: activeCard,
      secondChild: inactiveCard,
      crossFadeState: isFocused
          ? CrossFadeState.showFirst
          : CrossFadeState.showSecond,
      duration: sectionHideDuration,
      sizeCurve: sectionHideCurve,
    );
  }

  void markAddressShared(BuildContext context, AddressInfo address) async {
    final walletCtx = WalletContext.of(context)!;
    await walletCtx.wallet.markAddressShared(address.index);
    updateToIndex(address.index);
  }

  void copyAddress(BuildContext context, AddressInfo address) {
    copyAction(context, 'AddressInfo', address.address.toString());
    markAddressShared(context, address);
    focus = ReceivePageFocus.verify;
  }

  void showAddressQr(BuildContext context, AddressInfo address) async {
    if (mounted) focus = ReceivePageFocus.share;
    final theme = Theme.of(context).copyWith(
      colorScheme: ColorScheme.fromSeed(
        brightness: Brightness.light,
        seedColor: seedColor,
      ),
    );
    final img = addressQrImage(address);
    final isDone =
        await showDialog<bool>(
          context: context,
          barrierDismissible: true,
          builder: (context) {
            return Theme(
              data: theme,
              child: PopScope(
                canPop: false,
                child: Dialog(
                  child: ConstrainedBox(
                    constraints: BoxConstraints(maxWidth: 580),
                    child: SingleChildScrollView(
                      padding: EdgeInsets.all(16),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        spacing: 28,
                        children: [
                          AspectRatio(
                            aspectRatio: 1,
                            child: PrettyQrView(qrImage: img),
                          ),
                          FilledButton(
                            onPressed: () => Navigator.pop(context, true),
                            child: Text('Done'),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            );
          },
        ) ??
        false;
    if (isDone) {
      if (context.mounted) markAddressShared(context, address);
      if (mounted) focus = ReceivePageFocus.verify;
    }
  }

  void openAddressPicker(BuildContext context, AddressInfo address) {
    final walletCtx = WalletContext.of(context)!;
    showBottomSheetOrDialog(
      context,
      title: Text('Receive Addresses'),
      builder: (context, scrollController) {
        return walletCtx.wrap(
          AddressList(
            onTap: (context, addr) =>
                updateToIndex(addr.index, next: ReceivePageFocus.share),
            showUsed: address.used,
            scrollToDerivationIndex: address.index,
            scrollController: scrollController,
          ),
        );
      },
    );
  }

  Widget _dialogBodyBuilder(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisAlignment: MainAxisAlignment.center,
          spacing: 12,
          children: const [
            Flexible(flex: 1, child: Icon(Icons.send)),
            Flexible(
              flex: 3,
              child: Text(
                "Give the address you have scanned/copied to the sender.",
                softWrap: true,
              ),
            ),
          ],
        ),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisAlignment: MainAxisAlignment.center,
          spacing: 12,
          children: [
            Flexible(flex: 1, child: Icon(Icons.visibility)),
            Expanded(
              flex: 3,
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text:
                          "Confirm they have the same address as shown on the device's screen.\nMake sure the two ",
                    ),
                    TextSpan(
                      text: "highlighted",
                      style: TextStyle(color: theme.colorScheme.primary),
                    ),
                    TextSpan(text: " chunks are present."),
                  ],
                ),
                softWrap: true,
              ),
            ),
          ],
        ),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisAlignment: MainAxisAlignment.center,
          spacing: 12,
          children: const [
            Flexible(flex: 1, child: Icon(Icons.block)),
            Expanded(
              flex: 3,
              child: Text(
                "Do not send the bitcoin if it doesn't match.",
                softWrap: true,
              ),
            ),
          ],
        ),
      ],
    );
  }
}
