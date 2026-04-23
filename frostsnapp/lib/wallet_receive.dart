import 'dart:async';
import 'dart:collection';
import 'dart:ui';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
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
  FullscreenActionDialogController? _fullscreenDialogController;
  bool verificationSuccess = false;

  ReceivePageFocus _focus = ReceivePageFocus.share;
  bool _copyJustHit = false;
  Timer? _copyRevertTimer;
  ReceivePageFocus get focus => _focus;
  set focus(ReceivePageFocus v) {
    if (v == _focus || _address == null) return;
    if (v == ReceivePageFocus.verify) {
      _verifyStreamSub?.cancel();
      final targetDevices = widget.wallet
          .frostKey()!
          .accessStructures()
          .expand((as) => as.devices())
          .toSet();
      _fullscreenDialogController = _buildVerifyDialogController(targetDevices);
      // The protocol runs on the coordinator's background thread the moment
      // `verifyAddress` is called — the subscription isn't needed to keep it
      // alive, and cancellation goes through `coord.cancelProtocol()` below.
      // We still `.listen` so the stream is drained for logging/debug; the
      // emitted state (`targetDevices`, `connectedDevices`) duplicates info
      // we already derive synchronously, so we discard it.
      _verifyStreamSub = coord
          .verifyAddress(
            keyId: widget.wallet.keyId(),
            addressIndex: _address!.index,
          )
          .listen((_) {});

      setState(() {
        _focus = v;
      });
      return;
    }
    _fullscreenDialogController?.dispose();
    _fullscreenDialogController = null;
    if (_verifyStreamSub != null) coord.cancelProtocol();
    _verifyStreamSub?.cancel();
    _verifyStreamSub = null;
    setState(() {
      _focus = v;
    });
  }

  FullscreenActionDialogController _buildVerifyDialogController(
    Iterable<DeviceId> devices,
  ) {
    return FullscreenActionDialogController(
      context: context,
      devices: devices,
      title: 'Verify address on device',
      body: _dialogBodyBuilder,
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () {
            verificationSuccess = false;
            _fullscreenDialogController?.enabled = false;
          },
        ),
        OutlinedButton(
          onPressed: () {
            verificationSuccess = true;
            _fullscreenDialogController?.enabled = false;
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
    _fullscreenDialogController?.dispose();
    _copyRevertTimer?.cancel();
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
      title: Text('Share Address'),
      trailing: TextButton.icon(
        onPressed: _address == null
            ? null
            : () => openAddressPicker(context, _address!),
        label: Text(
          'Address #${_address?.index}',
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
                      : () async {
                          const beforeVerify = Duration(milliseconds: 300);
                          // Outlasts beforeVerify + cross-fade to avoid mid-transition flip.
                          const copyRevert = Duration(milliseconds: 800);
                          setState(() => _copyJustHit = true);
                          _copyRevertTimer?.cancel();
                          _copyRevertTimer = Timer(copyRevert, () {
                            if (mounted) {
                              setState(() => _copyJustHit = false);
                            }
                          });
                          await copyToClipboardQuietly(
                            _address!.address.toString(),
                          );
                          if (!mounted) return;
                          markAddressShared(context, _address!);
                          await Future.delayed(beforeVerify);
                          if (!mounted) return;
                          focus = ReceivePageFocus.verify;
                        },
                  label: Text(_copyJustHit ? 'Copied' : 'Copy'),
                  icon: Icon(
                    _copyJustHit ? Icons.check_rounded : Icons.copy_rounded,
                    color: _copyJustHit
                        ? Theme.of(context).colorScheme.primary
                        : null,
                  ),
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
                    title: Text('Verify Address'),
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
        title: Text('Verify Address'),
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
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        const Text(
          "Check the address you have copied to give to the sender against the address shown on your device.\n\nFor full verification, check directly with the sender.",
          softWrap: true,
        ),
        if (_address != null) ...[
          SizedBox(height: 8),
          Text(
            'Receive Address #${_address!.index}',
            style: Theme.of(context).textTheme.titleSmall,
          ),
          _BlurredAddress(address: spacedHex(_address!.address.toString())),
        ],
      ],
    );
  }
}

class _BlurredAddress extends StatefulWidget {
  final String address;

  const _BlurredAddress({required this.address});

  @override
  State<_BlurredAddress> createState() => _BlurredAddressState();
}

class _BlurredAddressState extends State<_BlurredAddress>
    with SingleTickerProviderStateMixin {
  bool _revealed = false;
  late final AnimationController _controller;
  late final Animation<double> _blurSigma;
  late final Animation<double> _overlayOpacity;
  late final Animation<double> _cautionOpacity;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(vsync: this, duration: Durations.medium4);
    final curved = CurvedAnimation(parent: _controller, curve: Curves.easeOut);
    _blurSigma = Tween(begin: 5.0, end: 0.0).animate(curved);
    _overlayOpacity = Tween(begin: 1.0, end: 0.0).animate(curved);
    _cautionOpacity = curved;
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _toggleReveal() {
    setState(() => _revealed = !_revealed);
    if (_revealed) {
      _controller.forward();
    } else {
      _controller.reverse();
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        GestureDetector(
          onTap: _toggleReveal,
          child: ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: AnimatedBuilder(
              animation: _blurSigma,
              builder: (context, child) {
                return Stack(
                  children: [
                    child!,
                    Positioned.fill(
                      child: BackdropFilter(
                        filter: ImageFilter.blur(
                          sigmaX: _blurSigma.value,
                          sigmaY: _blurSigma.value,
                        ),
                        child: FadeTransition(
                          opacity: _overlayOpacity,
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
        ),
        FadeTransition(
          opacity: _cautionOpacity,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            spacing: 8,
            children: [
              Icon(Icons.warning_amber_rounded, color: cautionColor, size: 20),
              Expanded(
                child: Text(
                  "Don't trust this screen — check where you have scanned or pasted the address against the address shown on your device.",
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: cautionColor,
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
