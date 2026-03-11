import 'dart:async';
import 'dart:collection';
import 'dart:ui';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
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

  StreamSubscription<VerifyAddressProtocolState>? _verifyStreamSub;
  late final FullscreenActionDialogController fullscreenDialogController;
  bool verificationSuccess = false;

  ReceivePageFocus _focus = ReceivePageFocus.share;
  ReceivePageFocus get focus => _focus;
  set focus(ReceivePageFocus v) {
    if (v == _focus || _address == null) return;
    if (v == ReceivePageFocus.verify) {
      _dialogAddressRevealed = false;
      _verifyStreamSub?.cancel();
      _verifyStreamSub = coord
          .verifyAddress(
            keyId: widget.wallet.keyId(),
            addressIndex: _address!.index,
          )
          .listen((state) {
            fullscreenDialogController.batchAddActionNeeded(
              context,
              state.connectedDevices,
            );
            fullscreenDialogController.clearAllExcept(state.connectedDevices);
          });

      setState(() {
        _focus = v;
      });
      return;
    }
    if (_verifyStreamSub != null) coord.cancelProtocol();
    _verifyStreamSub?.cancel();
    _verifyStreamSub = null;
    setState(() {
      _focus = v;
    });
  }

  bool get isRevealed => _address?.revealed ?? false;
  bool get isUsed => _address?.used ?? false;

  AddressInfo? _address;
  bool get isReady => _address != null;
  Wallet get wallet => widget.wallet;

  static const tilePadding = EdgeInsets.symmetric(horizontal: 16);
  static const tileShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(28)),
  );
  static const sectionPadding = EdgeInsets.fromLTRB(16, 0, 16, 16);
  static const sectionHideDuration = Durations.medium4;
  static const sectionHideCurve = Curves.easeInOutCubicEmphasized;

  QrImage addressQrImage(AddressInfo address) {
    final qrCode = QrCode.fromData(
      // the BIP recommends uppercasing the the string before going into the QR
      // so it can pack it more efficiently but it doesn't seem to make too much
      // difference.
      data: address.address.bip21Uri(),
      errorCorrectLevel: QrErrorCorrectLevel.M,
    );
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
    _verifyStreamSub?.cancel();
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
      shape: cardShape(context),
      margin: sectionPadding,
      color: theme.colorScheme.surfaceContainerHigh,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [header, cardBody],
      ),
    );
    final inactiveCard = Card.outlined(
      shape: cardShape(context),
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
      borderRadius: BorderRadius.circular(28),
      gradientColors: [
        theme.colorScheme.outlineVariant,
        theme.colorScheme.primary,
        theme.colorScheme.secondary,
        theme.colorScheme.tertiary,
      ],
      child: Card.filled(
        margin: EdgeInsets.zero,
        color: theme.colorScheme.surfaceContainerHigh,
        shape: cardShape(context),
        child: _verifyStreamSub == null
            ? null
            : Column(
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
                    padding: sectionPadding.copyWith(top: 20, bottom: 36),
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
                    child: OutlinedButton(
                      onPressed: () => focus = ReceivePageFocus.awaitTx,
                      child: Text('Skip'),
                    ),
                  ),
                ],
              ),
      ),
    );
    final inactiveCard = Card.outlined(
      shape: cardShape(context),
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
      shape: cardShape(context),
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
      shape: cardShape(context),
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

  bool _dialogAddressRevealed = false;
  final _blurredAddressKey = GlobalKey<_BlurredAddressState>();

  Widget _dialogBodyBuilder(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        const Text(
          "Check the address where you scanned or pasted it against the address shown on your device screen.",
          softWrap: true,
        ),
        if (_address != null) ...[
          SizedBox(height: 8),
          Text(
            'Receive Address #${_address!.index}',
            style: Theme.of(context).textTheme.titleSmall,
          ),
          _BlurredAddress(
            key: _blurredAddressKey,
            address: spacedHex(_address!.address.toString()),
            revealed: _dialogAddressRevealed,
            onReveal: () {
              _dialogAddressRevealed = !_dialogAddressRevealed;
              fullscreenDialogController.rebuild();
            },
          ),
          _blurredAddressKey.currentState?.buildCaution(context) ??
              Opacity(
                opacity: 0,
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  spacing: 8,
                  children: [
                    Icon(Icons.warning_amber_rounded, size: 20),
                    Expanded(
                      child: Text(
                        'Check the address shown on the device against where '
                        'you scanned or pasted it, or for full verification '
                        'check it with the sender — not here.',
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                    ),
                  ],
                ),
              ),
        ],
      ],
    );
  }
}

class _BlurredAddress extends StatefulWidget {
  final String address;
  final bool revealed;
  final VoidCallback onReveal;

  const _BlurredAddress({
    super.key,
    required this.address,
    required this.revealed,
    required this.onReveal,
  });

  @override
  State<_BlurredAddress> createState() => _BlurredAddressState();
}

class _BlurredAddressState extends State<_BlurredAddress>
    with TickerProviderStateMixin {
  late final AnimationController _blurController;
  late final AnimationController _cautionController;
  late final Animation<double> _blurAnimation;
  late final Animation<double> _overlayOpacity;

  @override
  void initState() {
    super.initState();
    _blurController = AnimationController(
      vsync: this,
      duration: Durations.medium4,
      value: widget.revealed ? 1.0 : 0.0,
    );
    _cautionController = AnimationController(
      vsync: this,
      duration: Durations.medium2,
      value: widget.revealed ? 1.0 : 0.0,
    );
    _blurAnimation = Tween(
      begin: 5.0,
      end: 0.0,
    ).animate(CurvedAnimation(parent: _blurController, curve: Curves.easeOut));
    _overlayOpacity = Tween(
      begin: 1.0,
      end: 0.0,
    ).animate(CurvedAnimation(parent: _blurController, curve: Curves.easeOut));
  }

  @override
  void didUpdateWidget(_BlurredAddress oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.revealed != oldWidget.revealed) {
      if (widget.revealed) {
        _cautionController.stop();
        _blurController.forward().then(
          (_) => Future.delayed(const Duration(milliseconds: 400), () {
            if (mounted && widget.revealed) _cautionController.forward();
          }),
        );
      } else {
        _cautionController.reverse();
        _blurController.reverse();
      }
    }
  }

  @override
  void dispose() {
    _blurController.dispose();
    _cautionController.dispose();
    super.dispose();
  }

  Widget buildCaution(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedBuilder(
      animation: _cautionController,
      builder: (context, child) =>
          Opacity(opacity: _cautionController.value, child: child),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        spacing: 8,
        children: [
          Icon(Icons.warning_amber_rounded, color: cautionColor, size: 20),
          Expanded(
            child: Text(
              'Check the address shown on the device against where '
              'you scanned or pasted it, or for full verification '
              'check it with the sender — not here.',
              style: theme.textTheme.bodySmall?.copyWith(color: cautionColor),
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return GestureDetector(
      onTap: widget.onReveal,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(12),
        child: AnimatedBuilder(
          animation: _blurController,
          builder: (context, child) {
            return Stack(
              children: [
                child!,
                Positioned.fill(
                  child: BackdropFilter(
                    filter: ImageFilter.blur(
                      sigmaX: _blurAnimation.value,
                      sigmaY: _blurAnimation.value,
                    ),
                    child: Opacity(
                      opacity: _overlayOpacity.value,
                      child: Container(
                        color: Colors.black.withValues(alpha: 0.1),
                        alignment: Alignment.center,
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          spacing: 8,
                          children: [
                            Icon(
                              Icons.visibility_off,
                              color: theme.colorScheme.onSurface,
                            ),
                            Text(
                              'Tap to reveal address',
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.colorScheme.onSurface,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            );
          },
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Text(
              widget.address,
              style: monospaceTextStyle.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
