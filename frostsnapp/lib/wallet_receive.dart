import 'dart:collection';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/wallet_tx_details.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:rxdart/rxdart.dart';

import 'global.dart';

class AddressList extends StatefulWidget {
  final ScrollController? scrollController;
  final bool showUsed;
  final Function(BuildContext, Address) onTap;
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
  List<Address> _addresses = [];
  List<Address> get addresses => _addresses;

  final _firstAddrKey = GlobalKey();
  late final ScrollController? _scrollController;
  ScrollController get scrollController =>
      widget.scrollController ?? _scrollController!;

  void update(BuildContext context, {void Function()? andSetState}) async {
    final walletCtx = WalletContext.of(context);
    if (walletCtx != null) {
      await walletCtx.superWallet.nextUnusedAddress(
        masterAppkey: walletCtx.masterAppkey,
      );
      final addresses = walletCtx.superWallet.addressesState(
        masterAppkey: walletCtx.masterAppkey,
      );
      if (mounted) {
        setState(() {
          _addresses = addresses;
          if (andSetState != null) andSetState();
        });
      }
    }
  }

  void deriveNewAddress(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;

    await walletCtx.superWallet.nextAddress(
      masterAppkey: walletCtx.masterAppkey,
    );
    if (context.mounted) {
      update(context);
      await scrollController.animateTo(
        0.0,
        duration: Durations.long4,
        curve: Curves.easeInOutCubicEmphasized,
      );
    }
  }

  @override
  void initState() {
    super.initState();
    _scrollController =
        widget.scrollController == null ? ScrollController() : null;

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

  Widget buildAddressItem(BuildContext context, Address addr, {Key? key}) {
    final theme = Theme.of(context);
    final usedTag = Card.outlined(
      color: Colors.transparent,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
        child: Text('Used', style: TextStyle(color: theme.colorScheme.error)),
      ),
    );
    final freshTag = Card.outlined(
      color: Colors.transparent,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 8),
        child: Text('Fresh', style: TextStyle(color: Colors.greenAccent)),
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
          color:
              addr.used ? theme.colorScheme.primary : theme.colorScheme.primary,
          fontFamily: monospaceTextStyle.fontFamily,
        ),
      ),
      title: Text(
        spacedHex(addr.addressString),
        style: monospaceTextStyle.copyWith(
          color: addr.used ? theme.colorScheme.onSurfaceVariant : null,
        ),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 4,
        children: [
          switch ((addr.used, addr.shared)) {
            (true, _) => usedTag,
            (_, true) => SizedBox(),
            (false, false) => freshTag,
          },
          Icon(Icons.chevron_right_rounded),
        ],
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
              children:
                  addresses
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
  final int? derivationIndex;

  const ReceivePage({
    super.key,
    this.scrollController,
    required this.wallet,
    this.derivationIndex,
  });

  @override
  State<ReceivePage> createState() => _ReceiverPageState();
}

class _ReceiverPageState extends State<ReceivePage> {
  BehaviorSubject<VerifyAddressProtocolState>? _verifyStream;

  ReceivePageFocus _focus = ReceivePageFocus.share;
  ReceivePageFocus get focus => _focus;
  set focus(ReceivePageFocus v) {
    if (v == _focus || _address == null) return;
    if (v == ReceivePageFocus.verify) {
      final stream =
          coord
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

  bool get isShared => _address?.shared ?? false;
  bool get isUsed => _address?.used ?? false;
  bool get isFresh => _address?.fresh ?? false;

  Address? _address;
  bool get isReady => _address != null;
  Wallet get wallet => widget.wallet;

  static const tilePadding = EdgeInsets.symmetric(horizontal: 20);
  static const tileShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(12)),
  );
  static const sectionPadding = EdgeInsets.fromLTRB(20, 0, 20, 20);
  static const sectionHideDuration = Durations.medium4;
  static const sectionHideCurve = Curves.easeInOutCubicEmphasized;

  QrImage addressQrImage(Address address) {
    final qrCode = QrCode(8, QrErrorCorrectLevel.L);
    qrCode.addData(address.addressString);
    return QrImage(qrCode);
  }

  @override
  void initState() {
    super.initState();

    final index = widget.derivationIndex;
    (index != null) ? updateToIndex(index) : updateToNextUnused();
  }

  @override
  void dispose() {
    super.dispose();
    if (_focus == ReceivePageFocus.verify) {
      coord.cancelProtocol();
    }
  }

  void updateToIndex(int index) {
    final addr = wallet.superWallet.addressState(
      masterAppkey: wallet.masterAppkey,
      index: index,
    );
    if (mounted) setState(() => _address = addr);
  }

  void updateToNextUnused() async {
    final addr = await wallet.superWallet.nextUnusedAddress(
      masterAppkey: wallet.masterAppkey,
    );
    if (mounted) {
      setState(() {
        _address = addr;
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
        SliverToBoxAdapter(child: buildAwaitTxCard(context)),
        SliverSafeArea(
          sliver: SliverPadding(padding: EdgeInsets.only(bottom: 4)),
        ),
      ],
    );
  }

  Widget buildShareCard(BuildContext context) {
    final isFocused = focus == ReceivePageFocus.share;

    final theme = Theme.of(context);
    final subtitle = Text(isShared ? 'Shared address' : 'Fresh address');
    final header = ListTile(
      shape: tileShape,
      contentPadding: tilePadding.copyWith(right: 8),
      title: Text('Share'),
      subtitle: isFocused ? subtitle : null,
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!isFocused) subtitle,
          TextButton.icon(
            onPressed:
                _address == null
                    ? null
                    : () => openAddressPicker(context, _address!),
            label: Text(
              '#${_address?.index}',
              style: monospaceTextStyle.copyWith(
                decoration: isUsed ? TextDecoration.lineThrough : null,
              ),
            ),
            icon: Icon(Icons.arrow_drop_down_rounded),
          ),
        ],
      ),
      onTap: switch (focus) {
        ReceivePageFocus.share => null,
        _ => () => focus = ReceivePageFocus.share,
      },
    );
    final addressText = Text(
      spacedHex(_address?.addressString ?? ''),
      style: theme.textTheme.labelLarge?.copyWith(
        fontFamily: monospaceTextStyle.fontFamily,
        color: theme.colorScheme.onSurfaceVariant,
      ),
      textAlign: TextAlign.start,
    );

    final cardBody = Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Padding(padding: tilePadding, child: addressText),
        Padding(
          padding: EdgeInsets.symmetric(vertical: 12, horizontal: 20),
          child: Row(
            spacing: 12,
            children: [
              Expanded(
                child: FilledButton.tonalIcon(
                  onPressed:
                      _address == null
                          ? null
                          : () => copyAddress(context, _address!),
                  label: Text('Copy'),
                  icon: Icon(Icons.copy_rounded),
                ),
              ),
              Expanded(
                child: FilledButton.tonalIcon(
                  onPressed:
                      _address == null
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
      crossFadeState:
          isFocused ? CrossFadeState.showFirst : CrossFadeState.showSecond,
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
        child:
            _verifyStream == null
                ? null
                : StreamBuilder(
                  stream: _verifyStream,
                  builder: (context, targetDevicesSnapshot) {
                    final targetDevices = deviceIdSet(
                      targetDevicesSnapshot.data?.targetDevices ?? [],
                    );

                    final dismissButton = OutlinedButton(
                      onPressed: () => focus = ReceivePageFocus.awaitTx,
                      child: Text('Dismiss'),
                    );

                    return StreamBuilder(
                      stream: deviceListSubject,
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

                        return Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            ListTile(
                              shape: tileShape,
                              title: Text('Verify'),
                              subtitle:
                                  displayingDevices.isEmpty
                                      ? Text('Plug in a device to continue.')
                                      : Text(
                                        'Verify that the pasted or scanned address matches the device display.',
                                      ),
                              isThreeLine: displayingDevices.isNotEmpty,
                              contentPadding: tilePadding,
                              minVerticalPadding: 16,
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

  Widget buildAwaitTxCard(BuildContext context) {
    final thisAddr = _address?.addressString;
    final walletCtx = WalletContext.of(context)!;
    final isFocused = focus == ReceivePageFocus.awaitTx;
    final theme = Theme.of(context);
    return StreamBuilder(
      stream: walletCtx.txStream,
      builder: (context, snapshot) {
        final now = DateTime.now();
        final chainTipHeight = walletCtx.wallet.superWallet.height();
        final relevantTxs =
            (snapshot.data?.txs ?? []).where((tx) {
              if (thisAddr == null) return false;
              final hasReceivedToThisAddress = tx.recipients
                  .map((a) => a.address(network: walletCtx.network))
                  .nonNulls
                  .any((addr) => thisAddr == addr);
              return hasReceivedToThisAddress;
            }).toList();
        final txTiles = relevantTxs.map((tx) {
          final txDetails = TxDetailsModel(
            tx: tx,
            chainTipHeight: chainTipHeight,
            now: now,
          );
          return TxSentOrReceivedTile(
            txDetails: txDetails,
            onTap:
                () => showBottomSheetOrDialog(
                  context,
                  titleText: 'Transaction Details',
                  builder:
                      (context, scrollController) => walletCtx.wrap(
                        TxDetailsPage(
                          scrollController: scrollController,
                          txStates: walletCtx.txStream,
                          txDetails: txDetails,
                        ),
                      ),
                ),
          );
        });

        final subtitle = Text(switch (relevantTxs.length) {
          1 => '1 transaction received',
          _ => '${relevantTxs.length} transactions received',
        });
        final header = ListTile(
          shape: tileShape,
          contentPadding: tilePadding,
          title: Text('Receive'),
          subtitle: isFocused ? subtitle : null,
          trailing: isFocused ? null : subtitle,
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
              ...txTiles,
              if (relevantTxs.isEmpty)
                Padding(
                  padding: EdgeInsets.fromLTRB(24, 4, 24, 24),
                  child: Text(
                    'Waiting for transactions...',
                    textAlign: TextAlign.center,
                    style: TextStyle(color: theme.colorScheme.outline),
                  ),
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
          crossFadeState:
              isFocused ? CrossFadeState.showFirst : CrossFadeState.showSecond,
          duration: sectionHideDuration,
          sizeCurve: sectionHideCurve,
        );
      },
    );
  }

  void markAddressShared(BuildContext context, Address address) async {
    final walletCtx = WalletContext.of(context)!;
    await walletCtx.superWallet.markAddressShared(
      masterAppkey: walletCtx.masterAppkey,
      derivationIndex: address.index,
    );
    updateToIndex(address.index);
  }

  void copyAddress(BuildContext context, Address address) {
    copyAction(context, 'Address', address.addressString);
    markAddressShared(context, address);
    focus = ReceivePageFocus.verify;
  }

  void showAddressQr(BuildContext context, Address address) async {
    if (mounted) focus = ReceivePageFocus.share;
    final theme = Theme.of(context).copyWith(
      colorScheme: ColorScheme.fromSeed(
        brightness: Brightness.light,
        seedColor: seedColor,
      ),
    );
    final img = addressQrImage(address);
    final isScanned = await showDialog<bool>(
      context: context,
      builder: (context) {
        return BackdropFilter(
          filter: blurFilter,
          child: Theme(
            data: theme,
            child: Dialog(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxWidth: 580),
                child: Padding(
                  padding: EdgeInsets.all(20),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    spacing: 16,
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
    );
    if (isScanned ?? false) {
      if (context.mounted) markAddressShared(context, address);
      if (mounted) focus = ReceivePageFocus.verify;
    }
  }

  void openAddressPicker(BuildContext context, Address address) {
    final walletCtx = WalletContext.of(context)!;
    showBottomSheetOrDialog(
      context,
      titleText: 'Pick Address',
      builder: (context, scrollController) {
        return walletCtx.wrap(
          AddressList(
            onTap: (context, addr) => updateToIndex(addr.index),
            showUsed: address.used,
            scrollToDerivationIndex: address.index,
            scrollController: scrollController,
          ),
        );
      },
    );
  }
}
