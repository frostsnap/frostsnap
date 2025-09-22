import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_list.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_manager.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_add.dart';
import 'package:frostsnap/wallet_list_controller.dart';
import 'package:frostsnap/wallet_more.dart';
import 'package:frostsnap/wallet_receive.dart';
import 'package:frostsnap/wallet_send.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:rxdart/rxdart.dart';

class Wallet {
  final SuperWallet superWallet;
  final MasterAppkey masterAppkey;

  Wallet({required this.superWallet, required this.masterAppkey});

  FrostKey? frostKey() {
    return coord.getFrostKey(keyId: keyId());
  }

  KeyId keyId() {
    return masterAppkey.keyId();
  }

  AddressInfo nextAddress() {
    return superWallet.nextAddress(masterAppkey: masterAppkey);
  }

  AddressInfo? getAddressInfo(int index) {
    return superWallet.getAddressInfo(masterAppkey: masterAppkey, index: index);
  }

  List<AddressInfo> addressesState() {
    return superWallet.addressesState(masterAppkey: masterAppkey);
  }

  markAddressShared(int index) async {
    return superWallet.markAddressShared(
      masterAppkey: masterAppkey,
      derivationIndex: index,
    );
  }
}

class WalletHome extends StatelessWidget {
  static const noWalletBodyCenterKey = Key('noWalletCenter');

  const WalletHome({super.key});

  Widget buildNoWalletBody(BuildContext context) {
    final theme = Theme.of(context);
    final sizeClass = WindowSizeContext.of(context);
    final alignTop =
        sizeClass == WindowSizeClass.compact ||
        sizeClass == WindowSizeClass.medium;
    return Align(
      key: Key('no-wallet-body'),
      alignment: alignTop ? Alignment.topCenter : Alignment(0, -0.25),
      child: SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: 460),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            spacing: 16,
            children: [
              SizedBox(
                height: 64,
                child: Image(
                  color: theme.colorScheme.primary,
                  image: AssetImage('assets/frostsnap-logo-trimmed.png'),
                ),
              ),
              WalletAddColumn(onPressed: makeOnPressed(context)),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final homeCtx = HomeContext.of(context)!;
    final walletListController = homeCtx.walletListController;
    final scaffoldKey = homeCtx.scaffoldKey;

    final bottomBar = ListenableBuilder(
      listenable: walletListController,
      builder: (context, _) {
        final bar = WalletBottomBar();
        return switch (walletListController.selected) {
          WalletItemKey item => item.tryWrapInWalletContext(
            context: context,
            child: bar,
          ),
          _ => bar,
        };
      },
    );

    final mediaSize = MediaQuery.sizeOf(context);
    final isNarrowDisplay = mediaSize.width < 840;
    final drawer = WalletDrawer(
      scaffoldKey: scaffoldKey,
      isRounded: isNarrowDisplay,
    );

    final scaffold = ListenableBuilder(
      listenable: walletListController,
      builder: (context, _) {
        final Widget body;
        if (!walletListController.gotInitialData) {
          body = Center(child: CircularProgressIndicator());
        } else {
          final selected = walletListController.selected;
          if (selected == null) {
            body = buildNoWalletBody(context);
          } else {
            body = switch (selected) {
              WalletItemKey item => item.tryWrapInWalletContext(
                key: Key('wrapped-${item.frostKey.keyId().toHex()}'),
                context: context,
                child: TxList(key: Key(item.frostKey.keyId().toHex())),
              ),
              WalletItemRestoration item => WalletRecoveryPage(
                key: Key(item.restoringKey.restorationId.toHex()),
                restoringKey: item.restoringKey,
                onWalletRecovered: (accessStructureRef) async {
                  walletListController.selectWallet(accessStructureRef.keyId);

                  final accessStructure = coord.getAccessStructure(
                    asRef: accessStructureRef,
                  )!; // we just made this access structure
                  final devices = accessStructure.devices();
                  final nonceRequest = await coord.createNonceRequest(
                    devices: devices,
                  );
                  if (nonceRequest.someNoncesRequested()) {
                    await MaybeFullscreenDialog.show<bool>(
                      context: context,
                      child: NonceReplenishDialog(
                        stream: coord
                            .replenishNonces(
                              nonceRequest: nonceRequest,
                              devices: devices,
                            )
                            .toBehaviorSubject(),
                        onCancel: () {
                          coord.cancelProtocol();
                          Navigator.pop(context, false);
                        },
                      ),
                    );
                  }
                },
              ),
            };
          }
        }

        return Scaffold(
          key: scaffoldKey,
          extendBody: true,
          resizeToAvoidBottomInset: true,
          drawer: isNarrowDisplay ? drawer : null,
          appBar: walletListController.selected == null
              ? AppBar(forceMaterialTransparency: true)
              : null,
          body: AnimatedSwitcher(
            duration: Durations.long1,
            reverseDuration: Duration.zero,
            switchInCurve: Curves.easeInOutCubicEmphasized,
            transitionBuilder: (child, animation) => SlideTransition(
              position: Tween<Offset>(
                begin: Offset(1, 0),
                end: Offset(0, 0),
              ).animate(animation),
              child: FadeTransition(
                opacity: CurvedAnimation(
                  parent: animation,
                  curve: Curves.linear,
                ),
                child: child,
              ),
            ),
            child: body,
          ),
          bottomNavigationBar: bottomBar,
        );
      },
    );

    return Row(
      children: [
        AnimatedSize(
          duration: Durations.medium4,
          curve: Curves.easeInOutCubicEmphasized,
          child: isNarrowDisplay ? const SizedBox.shrink() : drawer,
        ),
        Flexible(child: scaffold),
      ],
    );
  }
}

class FloatingProgress extends StatefulWidget {
  final Stream<double> progressStream;

  const FloatingProgress({super.key, required this.progressStream});

  @override
  State<FloatingProgress> createState() => _FloatingProgress();
}

class _FloatingProgress extends State<FloatingProgress>
    with SingleTickerProviderStateMixin {
  late AnimationController _progressFadeController;
  double progress = 0.0;

  @override
  initState() {
    super.initState();
    _progressFadeController = AnimationController(
      vsync: this,
      duration: Duration(seconds: 2),
    );
    widget.progressStream.listen(
      (event) {
        if (!context.mounted) return;
        setState(() => progress = event);
      },
      onDone: () {
        if (!context.mounted) return;
        // trigger rebuild to start the animation
        setState(() => _progressFadeController.forward());
      },
    );
  }

  @override
  void dispose() {
    _progressFadeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Container(
        alignment: Alignment.center,
        child: AnimatedOpacity(
          opacity: _progressFadeController.isAnimating ? 0.0 : 1.0,
          duration: _progressFadeController.duration!,
          child: LinearProgressIndicator(value: progress),
        ),
      ),
    );
  }
}

class TxList extends StatefulWidget {
  const TxList({super.key});
  @override
  State<TxList> createState() => _TxListState();
}

class _TxListState extends State<TxList> {
  final scrollController = ScrollController();
  final atTopNotifier = ValueNotifier(true);

  /// Medium: 48.0, Large: 88.0
  static const offSetTrigger = 48.0;

  @override
  void initState() {
    super.initState();
    scrollController.addListener(() {
      if (!context.mounted) return;
      atTopNotifier.value = scrollController.offset <= offSetTrigger;
    }); // medium: 48.0, large: 88.0
  }

  @override
  void didUpdateWidget(covariant TxList oldWidget) {
    if (scrollController.hasClients) {
      atTopNotifier.value = scrollController.offset <= offSetTrigger;
    }
    super.didUpdateWidget(oldWidget);
  }

  @override
  void dispose() {
    scrollController.dispose();
    atTopNotifier.dispose();
    super.dispose();
  }

  KeyId? prevKey;

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final canCreate = OutgoingCountContext.of(context)!;
    final settingsCtx = SettingsContext.of(context)!;
    final fsCtx = FrostsnapContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);

    // TODO: This is a hack to scroll to top everytime we switch wallets.
    // There are better ways to do this but requires more involved changes.
    if (prevKey == null || !keyIdEquals(walletCtx.keyId, prevKey!)) {
      prevKey = walletCtx.keyId;
      if (scrollController.hasClients) scrollController.jumpTo(0.0);
    }

    const scrolledUnderElevation = 1.0;

    return CustomScrollView(
      controller: scrollController,
      physics: ClampingScrollPhysics(),
      slivers: <Widget>[
        SliverAppBar.medium(
          pinned: true,
          title: Text(frostKey?.keyName() ?? 'Unknown'),
          scrolledUnderElevation: scrolledUnderElevation,
          actionsPadding: EdgeInsets.only(right: 8),
          actions: [
            StreamBuilder(
              stream: settingsCtx.chainStatusStream(walletCtx.network),
              builder: (context, snap) {
                if (!snap.hasData) {
                  return SizedBox();
                }
                final chainStatus = snap.data!;
                return ChainStatusIcon(chainStatus: chainStatus);
              },
            ),
          ],
        ),
        PinnedHeaderSliver(
          child: UpdatingBalance(
            txStream: walletCtx.txStream,
            atTopNotifier: atTopNotifier,
            scrolledUnderElevation: scrolledUnderElevation,
            expandedHeight: 144.0,
            frostKey: frostKey,
          ),
        ),
        StreamBuilder(
          stream: MergeStream<void>([
            walletCtx.signingSessionSignals,
            // Also rebuild on canonical tx list changes since `unbroadcastedTxs` excludes from the
            // canonical tx list.
            walletCtx.txStream.map((_) => {}),
          ]),
          builder: (context, _) {
            final chainTipHeight = walletCtx.wallet.superWallet.height();
            final now = DateTime.now();
            final uncanonicalTiles = coord
                .uncanonicalTxs(
                  sWallet: walletCtx.wallet.superWallet,
                  masterAppkey: walletCtx.masterAppkey,
                )
                .map((uncanonicalTx) {
                  final txDetails = TxDetailsModel(
                    tx: uncanonicalTx.tx,
                    chainTipHeight: chainTipHeight,
                    now: now,
                  );
                  final session = uncanonicalTx.activeSession;
                  if (session != null) {
                    final signingState = session.state();
                    return TxSentOrReceivedTile(
                      onTap: () => showBottomSheetOrDialog(
                        context,
                        title: Text('Transaction Details'),
                        builder: (context, scrollController) => walletCtx.wrap(
                          TxDetailsPage.restoreSigning(
                            scrollController: scrollController,
                            txStates: walletCtx.txStream,
                            txDetails: txDetails,
                            signingSessionId: signingState.sessionId,
                            psbtMan: fsCtx.psbtManager,
                          ),
                        ),
                      ),
                      txDetails: txDetails,
                      signingState: signingState,
                    );
                  } else {
                    return TxSentOrReceivedTile(
                      onTap: () => showBottomSheetOrDialog(
                        context,
                        title: Text('Transaction Details'),
                        builder: (context, scrollController) => walletCtx.wrap(
                          TxDetailsPage.needsBroadcast(
                            scrollController: scrollController,
                            txStates: walletCtx.txStream,
                            txDetails: txDetails,
                            finishedSigningSessionId: uncanonicalTx.sessionId,
                            psbtMan: fsCtx.psbtManager,
                          ),
                        ),
                      ),
                      txDetails: txDetails,
                    );
                  }
                });

            // Avoid marking for rebuild while already rebuilding.
            WidgetsBinding.instance.addPostFrameCallback(
              (_) => canCreate.value = uncanonicalTiles.length,
            );

            return SliverVisibility(
              visible: uncanonicalTiles.isNotEmpty,
              sliver: SliverList.list(children: uncanonicalTiles.toList()),
            );
          },
        ),
        SliverSafeArea(
          top: false,
          sliver: StreamBuilder(
            stream: walletCtx.txStream,
            builder: (context, snapshot) {
              if (!snapshot.hasData) {
                return SliverToBoxAdapter(
                  child: Center(child: CircularProgressIndicator()),
                );
              }
              final transactions = snapshot.data?.txs ?? [];
              final chainTipHeight = walletCtx.wallet.superWallet.height();
              final now = DateTime.now();
              return SliverList.builder(
                itemCount: transactions.length,
                itemBuilder: (context, index) {
                  final txDetails = TxDetailsModel(
                    tx: transactions[index],
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
                },
              );
            },
          ),
        ),
      ],
    );
  }
}

class WalletDrawer extends StatelessWidget {
  final GlobalKey<ScaffoldState> scaffoldKey;
  final bool isRounded;

  const WalletDrawer({
    super.key,
    required this.scaffoldKey,
    this.isRounded = true,
  });

  /// Section header and divider padding as according to Material 3 specs.
  static const padding = EdgeInsetsDirectional.symmetric(
    horizontal: 28.0,
    vertical: 16.0,
  );

  Widget buildWalletDestination(BuildContext context, WalletItem item) {
    final theme = Theme.of(context);
    return NavigationDrawerDestination(
      icon: item.icon ?? Icon(Icons.wallet_rounded),
      label: SizedBox(
        width: 228,
        child: Padding(
          padding: const EdgeInsets.only(right: 24),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Expanded(
                child: Text(
                  item.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (!(item.network?.isMainnet() ?? true))
                (BuildContext context, {required String text}) {
                  return Text(
                    text,
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  );
                }(context, text: item.network?.name() ?? ''),
            ],
          ),
        ),
      ),
    );
  }

  static const outerRadius = Radius.circular(28.0);
  static const innerRadius = Radius.circular(8.0);
  static const topShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.only(
      topLeft: outerRadius,
      topRight: outerRadius,
      bottomLeft: innerRadius,
      bottomRight: innerRadius,
    ),
  );
  static const bottomShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.only(
      topLeft: innerRadius,
      topRight: innerRadius,
      bottomLeft: outerRadius,
      bottomRight: outerRadius,
    ),
  );
  static const midShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(innerRadius),
  );
  static const allShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(outerRadius),
  );
  static const tilePadding = EdgeInsets.symmetric(horizontal: 16, vertical: 6);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final controller = homeCtx.walletListController;
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        List<Widget> children = [
          SizedBox(
            height: 64,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(28, 0, 0, 8),
              child: Align(
                alignment: AlignmentDirectional.bottomStart,
                child: Text(
                  'Wallets',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
          ),
        ];
        children.addAll([
          ...controller.wallets.map(
            (item) => buildWalletDestination(context, item),
          ),
          NavigationDrawerDestination(
            icon: Icon(
              controller.selected == null
                  ? Icons.more_horiz_rounded
                  : Icons.add_rounded,
              color: controller.selected == null
                  ? null
                  : theme.colorScheme.primary,
            ),
            label: Text(
              'Create or restore',
              style: controller.selected == null
                  ? null
                  : TextStyle(color: theme.colorScheme.primary),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 28),
            child: Divider(
              height: 32,
              thickness: 1,
              color: theme.colorScheme.outlineVariant,
            ),
          ),
          NavigationDrawerDestination(
            icon: Icon(Icons.devices_rounded),
            label: SizedBox(
              width: 228,
              child: Padding(
                padding: const EdgeInsets.only(right: 24),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text('Connected Devices'),
                    StreamBuilder(
                      stream: GlobalStreams.deviceListSubject,
                      builder: (context, snapshot) {
                        final n = snapshot.data?.state.devices.length;
                        return n == null
                            ? SizedBox.shrink()
                            : Text(
                                '$n',

                                style: theme.textTheme.labelMedium?.copyWith(
                                  color: theme.colorScheme.onSurfaceVariant,
                                ),
                              );
                      },
                    ),
                  ],
                ),
              ),
            ),
          ),
          NavigationDrawerDestination(
            icon: Icon(Icons.settings_rounded),
            label: Text('Settings'),
          ),
        ]);

        final drawerColor = theme.colorScheme.surface;

        final drawer = NavigationDrawer(
          backgroundColor: drawerColor,
          onDestinationSelected: (index) async {
            final walletCount = controller.wallets.length;
            if (index < walletCount) {
              controller.selectedIndex = index;
              scaffoldKey.currentState?.closeDrawer();
            } else if (index == walletCount) {
              controller.selectedIndex = null;
              scaffoldKey.currentState?.closeDrawer();
            } else if (index == walletCount + 1) {
              await MaybeFullscreenDialog.show(
                context: context,
                barrierDismissible: true,
                child: homeCtx.wrap(DeviceListPage()),
              );
            } else if (index == walletCount + 2) {
              await Navigator.push(
                context,
                MaterialPageRoute(builder: (context) => SettingsPage()),
              );
            }
          },
          selectedIndex: controller.selectedIndex ?? controller.wallets.length,
          children: children,
        );

        final maybeContainedDrawer = isRounded
            ? drawer
            : Container(color: drawerColor, child: drawer);

        return maybeContainedDrawer;
      },
    );
  }
}

class WalletBottomBar extends StatelessWidget {
  const WalletBottomBar({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    // final fsCtx = FrostsnapContext.of(context)!;
    if (walletCtx == null) return SizedBox();
    final outgoingCount = OutgoingCountContext.of(context)!;

    final theme = Theme.of(context);

    final textButtonStyle = TextButton.styleFrom(
      fixedSize: Size.fromHeight(48),
      foregroundColor: theme.colorScheme.onPrimaryContainer,
    );

    final highlightTextButtonStyle = TextButton.styleFrom(
      fixedSize: Size.fromHeight(48),
      backgroundColor: theme.colorScheme.surfaceContainer,
      foregroundColor: theme.colorScheme.onSurface,
    );

    final iconButtonStyle = IconButton.styleFrom(
      fixedSize: Size.square(48),
      foregroundColor: theme.colorScheme.onPrimaryContainer,
    );

    final receiveButton = TextButton.icon(
      onPressed: () => showBottomSheetOrDialog(
        context,
        title: Text('Receive'),
        builder: (context, scrollController) => walletCtx.wrap(
          ReceivePage(
            wallet: walletCtx.wallet,
            txStream: walletCtx.txStream,
            scrollController: scrollController,
          ),
        ),
      ),
      label: Text('Receive'),
      icon: Icon(Icons.south_east),
      style: textButtonStyle,
    );

    final sendButton = ValueListenableBuilder(
      valueListenable: outgoingCount,
      builder: (context, value, _) {
        final button = TextButton.icon(
          onPressed: () async => await showPickOutgoingTxDialog(context),
          label: value == 0 ? Text('Send') : Text('Continue'),
          icon: Icon(Icons.north_east),
          style: value == 0 ? textButtonStyle : highlightTextButtonStyle,
        );

        return Badge.count(
          count: value,
          // Only show count badge if we have more than one uncanonical outgoing tx.
          isLabelVisible: value > 1,
          child: button,
        );
      },
    );

    final moreButton = IconButton(
      onPressed: () => showBottomSheetOrDialog(
        context,
        title: Text('More Actions'),
        builder: (context, scrollController) =>
            walletCtx.wrap(WalletMore(scrollController: scrollController)),
      ),
      icon: Icon(Icons.more_vert_rounded),
      style: iconButtonStyle,
      tooltip: 'More',
    );

    // Replicates a Material 3 Expressive Toolbar
    // https://m3.material.io/components/toolbars
    return SizedBox(
      // Arbitary height to bound the bottom bar.
      height: 200,
      child: SafeArea(
        child: Align(
          alignment: Alignment.bottomCenter,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: Material(
              color: theme.colorScheme.primaryContainer,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.all(Radius.circular(32)),
              ),
              elevation: 12,
              child: Padding(
                padding: const EdgeInsets.all(8),
                child: Row(
                  spacing: 4,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Flexible(child: receiveButton),
                    Flexible(child: sendButton),
                    Flexible(child: moreButton),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<void> showTxDetailsDialog(
    BuildContext context,
    UncanonicalTx uncanonicalTx,
  ) async {
    final walletCtx = WalletContext.of(context)!;
    final fsCtx = FrostsnapContext.of(context)!;

    final txDetails = TxDetailsModel(
      tx: uncanonicalTx.tx,
      chainTipHeight: walletCtx.wallet.superWallet.height(),
      now: DateTime.now(),
    );
    final session = uncanonicalTx.activeSession;
    if (session != null) {
      await showBottomSheetOrDialog(
        context,
        title: Text('Transaction Details'),
        builder: (context, scrollController) => walletCtx.wrap(
          TxDetailsPage.restoreSigning(
            scrollController: scrollController,
            txStates: walletCtx.txStream,
            txDetails: txDetails,
            signingSessionId: session.state().sessionId,
            psbtMan: fsCtx.psbtManager,
          ),
        ),
      );
    } else {
      await showBottomSheetOrDialog(
        context,
        title: Text('Transaction Details'),
        builder: (context, scrollController) => walletCtx.wrap(
          TxDetailsPage.needsBroadcast(
            scrollController: scrollController,
            txStates: walletCtx.txStream,
            txDetails: txDetails,
            finishedSigningSessionId: uncanonicalTx.sessionId,
            psbtMan: fsCtx.psbtManager,
          ),
        ),
      );
    }
  }

  Future<void> showPickOutgoingTxDialog(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;
    final uncanonicalTxs = coord.uncanonicalTxs(
      sWallet: walletCtx.wallet.superWallet,
      masterAppkey: walletCtx.masterAppkey,
    );

    if (uncanonicalTxs.length == 0) {
      await showBottomSheetOrDialog(
        context,
        title: Text('Send'),
        builder: (context, scrollController) =>
            walletCtx.wrap(WalletSendPage(scrollController: scrollController)),
      );
      return;
    }

    if (uncanonicalTxs.length == 1) {
      await showTxDetailsDialog(context, uncanonicalTxs.first);
      return;
    }

    final parentCtx = context;
    await showBottomSheetOrDialog(
      parentCtx,
      builder: (context, scrollController) {
        return CustomScrollView(
          controller: scrollController,
          shrinkWrap: true,
          physics: ClampingScrollPhysics(),
          slivers: [
            SliverSafeArea(
              sliver: SliverList.builder(
                itemCount: uncanonicalTxs.length,
                itemBuilder: (BuildContext context, int index) {
                  final uncanonicalTx = uncanonicalTxs[index];
                  final txDetails = TxDetailsModel(
                    tx: uncanonicalTx.tx,
                    chainTipHeight: walletCtx.superWallet.height(),
                    now: DateTime.now(),
                  );
                  return TxSentOrReceivedTile(
                    txDetails: txDetails,
                    signingState: uncanonicalTx.activeSession?.state(),
                    onTap: () {
                      Navigator.popUntil(context, (r) => r.isFirst);
                      showTxDetailsDialog(parentCtx, uncanonicalTx);
                    },
                  );
                },
              ),
            ),
          ],
        );
      },
    );
  }
}

class UpdatingBalance extends StatefulWidget {
  final ValueNotifier<bool> atTopNotifier;
  final Stream<TxState> txStream;
  final FrostKey? frostKey;
  final double? scrolledUnderElevation;
  final double expandedHeight;

  const UpdatingBalance({
    super.key,
    required this.atTopNotifier,
    required this.txStream,
    this.frostKey,
    this.scrolledUnderElevation,
    this.expandedHeight = 180.0,
  });

  @override
  State<UpdatingBalance> createState() => _UpdatingBalanceState();
}

class _UpdatingBalanceState extends State<UpdatingBalance> {
  int pendingIncomingBalance = 0;
  int avaliableBalance = 0;
  StreamSubscription? streamSub;

  @override
  void initState() {
    super.initState();
    streamSub = widget.txStream.listen(onData);
  }

  @override
  void didUpdateWidget(covariant UpdatingBalance oldWidget) {
    // TODO; To make this more performant, we can check to see if the KeyId has changed.
    streamSub?.cancel();
    streamSub = widget.txStream.listen(onData);
    super.didUpdateWidget(oldWidget);
  }

  @override
  void dispose() {
    streamSub?.cancel();
    super.dispose();
  }

  void onData(TxState txState) {
    if (context.mounted) {
      setState(() {
        pendingIncomingBalance = txState.untrustedPendingBalance;
        avaliableBalance = txState.balance;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final frostKey = widget.frostKey;

    final theme = Theme.of(context);
    final balanceTextStyle = theme.textTheme.headlineLarge;
    final pendingBalanceTextStyle = theme.textTheme.bodyLarge?.copyWith(
      color: theme.disabledColor,
    );

    final scrolledColor = ElevationOverlay.applySurfaceTint(
      theme.colorScheme.surfaceContainer,
      theme.colorScheme.surfaceTint,
      theme.appBarTheme.elevation ?? widget.scrolledUnderElevation ?? 3.0,
    );

    const duration = Durations.extralong4;
    const curve = Curves.easeInOutCubicEmphasized;

    final stack = ValueListenableBuilder(
      valueListenable: widget.atTopNotifier,
      builder: (context, atTop, _) => Stack(
        children: [
          Align(
            alignment: Alignment.topCenter,
            child: AnimatedContainer(
              duration: duration,
              curve: curve,
              height: atTop ? widget.expandedHeight / 2 : 0,
              color: atTop ? null : scrolledColor,
            ),
          ),
          AnimatedAlign(
            duration: duration,
            curve: curve,
            alignment: atTop ? Alignment.center : Alignment.topCenter,
            child: Container(
              color: atTop ? null : scrolledColor,
              padding: EdgeInsets.symmetric(
                horizontal: 24.0,
              ).copyWith(bottom: atTop ? (widget.expandedHeight / 10) : 20.0),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                mainAxisAlignment: MainAxisAlignment.start,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SatoshiText(
                    key: UniqueKey(),
                    value: avaliableBalance,
                    style: atTop
                        ? balanceTextStyle
                        : theme.textTheme.headlineSmall,
                    showSign: false,
                  ),
                  if (pendingIncomingBalance > 0)
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      mainAxisAlignment: MainAxisAlignment.end,
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Icon(
                          Icons.hourglass_top,
                          size: pendingBalanceTextStyle?.fontSize,
                          color: theme.disabledColor,
                        ),
                        SatoshiText(
                          value: pendingIncomingBalance,
                          showSign: true,
                          style: pendingBalanceTextStyle,
                          disabledColor: theme.colorScheme.outlineVariant,
                        ),
                      ],
                    ),
                ],
              ),
            ),
          ),
          if (frostKey != null)
            Align(
              alignment: Alignment.topLeft,
              child: BackupWarningBanner(frostKey: frostKey, shrink: !atTop),
            ),
        ],
      ),
    );

    return SizedBox(height: widget.expandedHeight, child: stack);
  }
}

class SatoshiText extends StatelessWidget {
  final int? value;
  final bool showSign;
  final bool hideLeadingWhitespace;
  final double letterSpacingReductionFactor;
  final TextStyle? style;
  final Color? disabledColor;
  final TextAlign align;

  const SatoshiText({
    super.key,
    required this.value,
    this.showSign = false,
    this.hideLeadingWhitespace = false,
    this.letterSpacingReductionFactor = 0.0,
    this.style,
    this.disabledColor,
    this.align = TextAlign.right,
  });

  const SatoshiText.withSign({Key? key, required int value})
    : this(key: key, value: value, showSign: true);

  @override
  Widget build(BuildContext context) {
    final baseStyle = DefaultTextStyle.of(context).style
        .merge(style)
        .copyWith(
          fontFamily: monospaceTextStyle.fontFamily,
          fontFeatures: [
            FontFeature.slashedZero(),
            FontFeature.tabularFigures(),
          ],
        );

    // We reduce the line spacing by the percentage from the fontSize (as per design specs).
    const defaultWordSpacingFactor = 0.36; // 0.32

    final baseLetterSpacing =
        (baseStyle.letterSpacing ?? 0.0) -
        (baseStyle.fontSize ?? 0.0) * letterSpacingReductionFactor;
    final wordSpacing =
        (baseStyle.letterSpacing ?? 0.0) -
        (baseStyle.fontSize ?? 0.0) * defaultWordSpacingFactor;

    final activeStyle = TextStyle(letterSpacing: baseLetterSpacing);
    final inactiveStyle = TextStyle(
      letterSpacing: baseLetterSpacing,
      color: disabledColor ?? Theme.of(context).disabledColor,
    );

    final value = this.value ?? 0;

    // Convert to BTC string with 8 decimal places
    String btcString = (value / 100000000.0).toStringAsFixed(8);
    // Split the string into two parts, removing - sign: before and after the decimal
    final parts = btcString.replaceFirst(r'-', '').split('.');
    final sign = value.isNegative ? '-' : (showSign ? '+' : '\u00A0');

    final unformatedWithoutSign =
        "${parts[0]}.${parts[1].substring(0, 2)} ${parts[1].substring(2, 5)} ${parts[1].substring(5)} \u20BF";
    final String unformatted;
    if (hideLeadingWhitespace && sign == '\u00A0') {
      unformatted = unformatedWithoutSign;
    } else {
      unformatted = '$sign $unformatedWithoutSign';
    }

    final activeIndex = () {
      var activeIndex = unformatted.indexOf(RegExp(r'[1-9]'));
      if (activeIndex == -1) activeIndex = unformatted.length - 1;
      return activeIndex;
    }();

    final List<TextSpan> spans = unformatted.characters.indexed.map((elem) {
      final (i, char) = elem;
      if (char == ' ') {
        return TextSpan(
          text: ' ',
          style: TextStyle(letterSpacing: wordSpacing),
        );
      }
      if (char == '+' || char == '-') {
        return TextSpan(text: char, style: activeStyle);
      }
      if (i < activeIndex) {
        return TextSpan(text: char, style: inactiveStyle);
      } else {
        return TextSpan(text: char, style: activeStyle);
      }
    }).toList();

    return Text.rich(
      TextSpan(children: spans),
      textAlign: align,
      softWrap: false,
      overflow: TextOverflow.fade,
      style: baseStyle,
    );
  }
}

Uri getBlockExplorer(BitcoinNetwork network) {
  if (network.isMainnet()) {
    return Uri.parse("https://mempool.space/");
  } else {
    // TODO: handle testnet properly
    return Uri.parse("https://mempool.space/signet/");
  }
}

class BackupWarningBanner extends StatelessWidget {
  final FrostKey frostKey;
  final bool shrink;

  const BackupWarningBanner({
    super.key,
    required this.frostKey,
    required this.shrink,
  });

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final backupStream = walletCtx.backupStream;
    final theme = Theme.of(context);

    final button = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8.0),
      child: IconButton(
        onPressed: () => onTap(context, walletCtx),
        icon: Icon(Icons.warning_rounded),
        style: IconButton.styleFrom(foregroundColor: theme.colorScheme.error),
        tooltip: 'This wallet has unfinished backups!',
      ),
    );

    final banner = ListTile(
      dense: true,
      contentPadding: EdgeInsets.symmetric(horizontal: 16),
      onTap: () => onTap(context, walletCtx),
      iconColor: theme.colorScheme.error,
      textColor: theme.colorScheme.error,
      leading: Icon(Icons.warning_rounded),
      trailing: Icon(Icons.chevron_right),
      title: Text('This wallet has unfinished backups!'),
    );

    final widget = shrink ? button : banner;

    final streamedBanner = StreamBuilder<BackupRun>(
      stream: backupStream,
      builder: (context, snapshot) {
        final backupRun = snapshot.data;
        final hideBanner = backupRun == null || isBackupDone(backupRun);
        return hideBanner ? SizedBox.shrink() : widget;
      },
    );

    return streamedBanner;
  }

  void onTap(BuildContext context, WalletContext walletContext) async {
    final backupManager = FrostsnapContext.of(context)!.backupManager;

    await MaybeFullscreenDialog.show(
      context: context,
      child: walletContext.wrap(
        BackupChecklist(
          backupManager: backupManager,
          accessStructure: frostKey.accessStructures()[0],
          showAppBar: true,
        ),
      ),
    );
  }

  bool isBackupDone(BackupRun backupRun) =>
      backupRun.devices.every((elem) => elem.$2 != null);
}
