import 'dart:async';
import 'package:flutter/material.dart' hide ConnectionState;
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/nostr_chat/chat_page.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/device_list.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/keygen.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/wallet_recovery_page.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_run.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
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
              WalletItemKey item => WalletModeShell(
                key: Key('shell-${item.frostKey.keyId().toHex()}'),
                walletItem: item,
              ),
              WalletItemRestoration item => WalletRecoveryPage(
                key: Key(item.restorationState.restorationId.toHex()),
                restorationState: item.restorationState,
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

/// Per-wallet shell: owns the Scaffold for a selected wallet and decides
/// between the local layout (today's flat TxList + bottom bar) and the
/// remote layout (tabbed Chat / Wallet) based on the persisted
/// `coordination_ui_enabled` flag.
class WalletModeShell extends StatelessWidget {
  const WalletModeShell({super.key, required this.walletItem});

  final WalletItemKey walletItem;

  @override
  Widget build(BuildContext context) {
    final keyHex = walletItem.frostKey.keyId().toHex();
    final asRef = walletItem.frostKey.accessStructures()[0].accessStructureRef();
    final nostr = NostrContext.of(context);
    return walletItem.tryWrapInWalletContext(
      key: Key('shell-context-$keyHex'),
      context: context,
      child: StreamBuilder<bool>(
        stream: nostr.watchCoordinationUi(asRef),
        initialData: nostr.isCoordinationUiEnabled(asRef),
        builder: (context, snap) {
          final isRemote = snap.data ?? false;
          if (isRemote) {
            return _RemoteWalletShell(
              key: Key('remote-$keyHex'),
              walletItem: walletItem,
              accessStructureRef: asRef,
            );
          }
          return _LocalWalletShell(
            key: Key('local-$keyHex'),
            walletItem: walletItem,
          );
        },
      ),
    );
  }
}

/// Today's flat layout: TxList body (which carries its own SliverAppBar) +
/// `[Receive, Send, More]` capsule at the bottom.
class _LocalWalletShell extends StatelessWidget {
  const _LocalWalletShell({super.key, required this.walletItem});
  final WalletItemKey walletItem;

  @override
  Widget build(BuildContext context) {
    final keyHex = walletItem.frostKey.keyId().toHex();
    return Scaffold(
      extendBody: true,
      resizeToAvoidBottomInset: true,
      body: TxList(key: Key('txlist-$keyHex')),
      bottomNavigationBar: WalletBottomBar(),
    );
  }
}

/// Chat-first remote-mode shell. The body is the channel chat; the
/// wallet's transaction history is reachable as a secondary route via
/// the AppBar's wallet/history action. There are no Send/Receive
/// actions — in-channel signing flows replace them and aren't designed
/// yet.
class _RemoteWalletShell extends StatefulWidget {
  const _RemoteWalletShell({
    super.key,
    required this.walletItem,
    required this.accessStructureRef,
  });

  final WalletItemKey walletItem;
  final AccessStructureRef accessStructureRef;

  @override
  State<_RemoteWalletShell> createState() => _RemoteWalletShellState();
}

class _RemoteWalletShellState extends State<_RemoteWalletShell> {
  late final ChatChromeController _chrome;
  late Future<ChannelConnectionParams> _channelParams;

  @override
  void initState() {
    super.initState();
    _chrome = ChatChromeController();
    _channelParams = _loadChannelParams();
  }

  @override
  void dispose() {
    _chrome.dispose();
    super.dispose();
  }

  Future<ChannelConnectionParams> _loadChannelParams() async {
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    return coord.channelConnectionParams(
      accessStructureRef: widget.accessStructureRef,
      encryptionKey: encryptionKey,
    );
  }

  Widget _buildConnectionIndicator(ConnectionState state) {
    final (color, tooltip) = switch (state) {
      ConnectionState_Connecting() => (Colors.orange, 'Connecting...'),
      ConnectionState_Connected() => (Colors.green, 'Connected'),
      ConnectionState_Disconnected(:final reason) => (
        Colors.red,
        'Disconnected${reason != null ? ': $reason' : ''}',
      ),
    };
    return Tooltip(
      message: tooltip,
      child: Container(
        width: 12,
        height: 12,
        decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      ),
    );
  }

  void _openWalletActivity(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => walletCtx.wrap(const RemoteWalletActivityPage()),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final homeCtx = HomeContext.of(context);
    final mediaSize = MediaQuery.sizeOf(context);
    final isNarrowDisplay = mediaSize.width < 840;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);
    final walletName = frostKey?.keyName() ?? 'Unknown';

    return Scaffold(
      extendBody: true,
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        leading: isNarrowDisplay
            ? IconButton(
                icon: const Icon(Icons.menu),
                onPressed: () =>
                    homeCtx?.scaffoldKey.currentState?.openDrawer(),
                tooltip: 'Open menu',
              )
            : null,
        title: Text(walletName),
        actions: [
          ListenableBuilder(
            listenable: _chrome,
            builder: (context, _) => Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _buildConnectionIndicator(_chrome.connectionState),
                const SizedBox(width: 8),
                IconButton(
                  icon: const Icon(Icons.receipt_long),
                  tooltip: 'Transaction history',
                  onPressed: () => _openWalletActivity(context),
                ),
                IconButton(
                  icon: const Icon(Icons.group),
                  tooltip: 'Group Info',
                  onPressed: _chrome.openGroupInfo,
                ),
              ],
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          if (frostKey != null)
            BackupWarningBanner(frostKey: frostKey, shrink: false),
          Expanded(
            child: FutureBuilder<ChannelConnectionParams>(
              future: _channelParams,
              builder: (context, snap) {
                if (snap.hasError) {
                  return _ChatLoadFailed(
                    error: snap.error!,
                    onRetry: () => setState(() {
                      _channelParams = _loadChannelParams();
                    }),
                  );
                }
                if (!snap.hasData) {
                  return const Center(child: CircularProgressIndicator());
                }
                return ChatPageBody(
                  accessStructureRef: widget.accessStructureRef,
                  walletName: walletName,
                  channelParams: snap.data!,
                  chrome: _chrome,
                  autofocus: false,
                );
              },
            ),
          ),
        ],
      ),
      backgroundColor: theme.colorScheme.surface,
    );
  }
}

/// Read-only wallet activity (balance + transaction history) for a
/// remote-coordinated wallet. Pushed as a secondary route from the
/// chat shell's wallet/history app-bar action; reuses
/// `walletTxSlivers` for the body.
class RemoteWalletActivityPage extends StatefulWidget {
  const RemoteWalletActivityPage({super.key});

  @override
  State<RemoteWalletActivityPage> createState() =>
      _RemoteWalletActivityPageState();
}

class _RemoteWalletActivityPageState extends State<RemoteWalletActivityPage> {
  final _scrollController = ScrollController();
  final _atTopNotifier = ValueNotifier<bool>(true);

  // Match the local-mode TxList threshold so the pinned balance card
  // collapses at the same scroll offset.
  static const _atTopThreshold = 48.0;

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(() {
      if (!mounted) return;
      _atTopNotifier.value = _scrollController.offset <= _atTopThreshold;
    });
  }

  @override
  void dispose() {
    _scrollController.dispose();
    _atTopNotifier.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final settingsCtx = SettingsContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);
    final walletName = frostKey?.keyName() ?? 'Unknown';
    return Scaffold(
      body: CustomScrollView(
        controller: _scrollController,
        physics: const ClampingScrollPhysics(),
        slivers: [
          SliverAppBar.medium(
            pinned: true,
            elevation: 0,
            surfaceTintColor: Colors.transparent,
            title: Text(walletName),
            actionsPadding: const EdgeInsets.only(right: 8),
            actions: [
              StreamBuilder(
                stream: settingsCtx.chainStatusStream(walletCtx.network),
                builder: (context, snap) {
                  if (!snap.hasData) return const SizedBox();
                  return ChainStatusIcon(chainStatus: snap.data!);
                },
              ),
            ],
          ),
          ...walletTxSlivers(
            context: context,
            atTopNotifier: _atTopNotifier,
          ),
        ],
      ),
    );
  }
}

/// Empty/error state for the chat surface when the channel-params
/// future fails (e.g. the encryption key fetch errored). Without an
/// explicit error path the FutureBuilder would show a permanent spinner.
class _ChatLoadFailed extends StatelessWidget {
  const _ChatLoadFailed({required this.error, required this.onRetry});

  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.error_outline,
              size: 48,
              color: theme.colorScheme.error,
            ),
            const SizedBox(height: 12),
            Text(
              'Couldn\'t open chat',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: 8),
            Text(
              '$error',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 16),
            FilledButton.tonalIcon(
              onPressed: onRetry,
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
            ),
          ],
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
    final settingsCtx = SettingsContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);

    // TODO: This is a hack to scroll to top everytime we switch wallets.
    // There are better ways to do this but requires more involved changes.
    if (prevKey == null || walletCtx.keyId != prevKey!) {
      prevKey = walletCtx.keyId;
      if (scrollController.hasClients) scrollController.jumpTo(0.0);
    }

    const scrolledUnderElevation = 1.0;
    final homeCtx = HomeContext.of(context);
    final isNarrowDisplay = MediaQuery.sizeOf(context).width < 840;

    return CustomScrollView(
      controller: scrollController,
      physics: ClampingScrollPhysics(),
      slivers: <Widget>[
        SliverAppBar.medium(
          pinned: true,
          elevation: 0,
          surfaceTintColor: Colors.transparent,
          // The wallet shell is a nested Scaffold without its own drawer;
          // wire the hamburger to the outer WalletHome's scaffold key on
          // narrow displays, otherwise Flutter would synthesise a back
          // button (no AppBar predecessor) instead of a menu icon.
          leading: isNarrowDisplay
              ? IconButton(
                  icon: const Icon(Icons.menu),
                  tooltip: 'Open menu',
                  onPressed: () =>
                      homeCtx?.scaffoldKey.currentState?.openDrawer(),
                )
              : null,
          automaticallyImplyLeading: false,
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
        ...walletTxSlivers(
          context: context,
          atTopNotifier: atTopNotifier,
          scrolledUnderElevation: scrolledUnderElevation,
        ),
      ],
    );
  }
}

/// The wallet body's slivers below the app bar — pinned balance, the
/// unbroadcasted-txs list, and the canonical tx history. Extracted from
/// `TxList` so a tabbed remote-wallet shell can mount them inside its own
/// `CustomScrollView` without doubling the app bar.
///
/// Caller supplies the [atTopNotifier] tracked against the enclosing
/// `CustomScrollView`'s scroll controller; the pinned balance reads it
/// to know when to expand vs collapse.
List<Widget> walletTxSlivers({
  required BuildContext context,
  required ValueNotifier<bool> atTopNotifier,
  double scrolledUnderElevation = 1.0,
}) {
  final walletCtx = WalletContext.of(context)!;
  final fsCtx = FrostsnapContext.of(context)!;
  final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);
  return <Widget>[
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
        final unbroadcastedTiles = coord
            .unbroadcastedTxs(
              sWallet: walletCtx.wallet.superWallet,
              masterAppkey: walletCtx.masterAppkey,
            )
            .map((unbroadcastedTx) {
              final txDetails = TxDetailsModel(
                tx: unbroadcastedTx.tx,
                chainTipHeight: chainTipHeight,
                now: now,
              );
              final session = unbroadcastedTx.activeSession;
              if (session != null) {
                final signingState = session.state();
                return TxSentOrReceivedTile(
                  onTap: () => showBottomSheetOrDialog(
                    context,
                    title: Text('Transaction Details'),
                    builder: (context, scrollController) => walletCtx.wrap(
                      TxDetailsPage(
                        scrollController: scrollController,
                        txStates: walletCtx.txStream,
                        txDetails: txDetails,
                        psbtMan: fsCtx.psbtManager,
                        signingParams: TxSigningParams.restore(
                          sessionId: signingState.sessionId,
                        ),
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
                        finishedSigningSessionId: unbroadcastedTx.sessionId,
                        psbtMan: fsCtx.psbtManager,
                      ),
                    ),
                  ),
                  txDetails: txDetails,
                );
              }
            });

        return SliverVisibility(
          visible: unbroadcastedTiles.isNotEmpty,
          sliver: SliverList.list(children: unbroadcastedTiles.toList()),
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
  ];
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
              controller.selected == null ? 'Home' : 'Create or restore',
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
      onPressed: () async {
        final backupRun = coord.getBackupRun(keyId: walletCtx.wallet.keyId());
        if (!backupRun.isComplete) {
          final frostKey = walletCtx.wallet.frostKey();
          if (frostKey != null) {
            final accessStructure = frostKey.accessStructures()[0];
            final choice = await showSecureWalletDialog(
              context,
              accessStructure,
            );
            if (choice != SecureWalletChoice.later) return;
          }
        }
        if (!context.mounted) return;
        showBottomSheetOrDialog(
          context,
          title: Text('Receive'),
          builder: (context, scrollController) => walletCtx.wrap(
            ReceivePage(
              wallet: walletCtx.wallet,
              txStream: walletCtx.txStream,
              scrollController: scrollController,
            ),
          ),
        );
      },
      label: Text('Receive', softWrap: false, overflow: TextOverflow.fade),
      icon: Icon(Icons.south_east),
      style: textButtonStyle,
    );

    final sendButton = StreamBuilder<int>(
      stream: walletCtx.unbroadcastedTxCount(),
      initialData: 0,
      builder: (context, snapshot) {
        final value = snapshot.data ?? 0;
        final hasOutgoing = value > 0;

        final button = TextButton.icon(
          onPressed: () async => await showPickOutgoingTxDialog(context),
          label: Text(
            hasOutgoing ? 'Continue' : 'Send',
            softWrap: false,
            overflow: TextOverflow.fade,
          ),
          icon: Icon(Icons.north_east),
          style: hasOutgoing ? highlightTextButtonStyle : textButtonStyle,
        );
        return Badge.count(
          count: value,
          // Only show count badge if we have more than one uncanonical outgoing tx.
          isLabelVisible: hasOutgoing,
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
    UnbroadcastedTx unbroadcastedTx,
  ) async {
    final walletCtx = WalletContext.of(context)!;
    final fsCtx = FrostsnapContext.of(context)!;

    final txDetails = TxDetailsModel(
      tx: unbroadcastedTx.tx,
      chainTipHeight: walletCtx.wallet.superWallet.height(),
      now: DateTime.now(),
    );
    final session = unbroadcastedTx.activeSession;
    if (session != null) {
      await showBottomSheetOrDialog(
        context,
        title: Text('Transaction Details'),
        builder: (context, scrollController) => walletCtx.wrap(
          TxDetailsPage(
            scrollController: scrollController,
            txStates: walletCtx.txStream,
            txDetails: txDetails,
            psbtMan: fsCtx.psbtManager,
            signingParams: TxSigningParams.restore(
              sessionId: session.state().sessionId,
            ),
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
            finishedSigningSessionId: unbroadcastedTx.sessionId,
            psbtMan: fsCtx.psbtManager,
          ),
        ),
      );
    }
  }

  Future<void> showPickOutgoingTxDialog(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;
    final unbroadcastedTxs = coord.unbroadcastedTxs(
      sWallet: walletCtx.wallet.superWallet,
      masterAppkey: walletCtx.masterAppkey,
    );

    if (unbroadcastedTxs.length == 0) {
      await showBottomSheetOrDialog(
        context,
        title: Text('Send'),
        builder: (context, scrollController) => walletCtx.wrap(
          WalletSendPage(
            scrollController: scrollController,
            superWallet: walletCtx.superWallet,
            masterAppkey: walletCtx.masterAppkey,
          ),
        ),
      );
      return;
    }

    if (unbroadcastedTxs.length == 1) {
      await showTxDetailsDialog(context, unbroadcastedTxs.first);
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
                itemCount: unbroadcastedTxs.length,
                itemBuilder: (BuildContext context, int index) {
                  final uncanonicalTx = unbroadcastedTxs[index];
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
    final settings = SettingsContext.of(context);

    final balanceTextStyle = theme.textTheme.headlineLarge;
    final pendingBalanceTextStyle = theme.textTheme.bodyLarge?.copyWith(
      color: theme.disabledColor,
    );

    final scrolledColor = ElevationOverlay.applySurfaceTint(
      theme.colorScheme.surfaceContainer,
      Colors.transparent,
      0.0,
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
                  InkWell(
                    borderRadius: BorderRadius.all(Radius.circular(8)),
                    child: SatoshiText(
                      value: avaliableBalance,
                      style: atTop
                          ? balanceTextStyle
                          : theme.textTheme.headlineSmall,
                      showSign: false,
                    ),
                    onTap: () {
                      final ss = settings!.settings;
                      ss.setHideBalance(value: !ss.hideBalance());
                    },
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
    final settings = SettingsContext.of(context)!;
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

    return StreamBuilder<DisplaySettings>(
      stream: settings.displaySettings,
      builder: (context, snapshot) {
        final hideBalance = snapshot.data?.hideBalance ?? false;

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
          // Replace digits with asterisks when hideBalance is true
          final displayChar = (hideBalance && RegExp(r'[0-9]').hasMatch(char))
              ? '*'
              : char;

          if (displayChar == ' ') {
            return TextSpan(
              text: ' ',
              style: TextStyle(letterSpacing: wordSpacing),
            );
          }
          if (displayChar == '+' || displayChar == '-') {
            return TextSpan(text: displayChar, style: activeStyle);
          }
          // When hiding balance, make all asterisks the same style to not reveal magnitude
          if (hideBalance && displayChar == '*') {
            return TextSpan(text: displayChar, style: activeStyle);
          }
          if (i < activeIndex) {
            return TextSpan(text: displayChar, style: inactiveStyle);
          } else {
            return TextSpan(text: displayChar, style: activeStyle);
          }
        }).toList();

        return Text.rich(
          TextSpan(children: spans),
          textAlign: align,
          softWrap: false,
          overflow: TextOverflow.fade,
          style: baseStyle,
        );
      },
    );
  }
}

Uri getBlockExplorer(BitcoinNetwork network) {
  if (network.isMainnet()) {
    return Uri.parse("https://mempool.space/");
  } else {
    // TODO: handle testnet properly
    return switch (network.name()) {
      "signet" => Uri.parse("https://mempool.space/signet/"),
      "testnet4" => Uri.parse("https://mempool.space/testnet4/"),
      "testnet" => Uri.parse("https://mempool.space/testnet/"),
      _ => Uri.parse("https://mempool.space/signet/"),
    };
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

    return StreamBuilder<BackupRun>(
      stream: backupStream,
      builder: (context, backupSnapshot) {
        final backupRun = backupSnapshot.data;
        final hideBanner = backupRun == null || backupRun.isComplete;
        if (hideBanner) return SizedBox.shrink();

        return StreamBuilder<TxState>(
          stream: walletCtx.txStream,
          builder: (context, txSnapshot) {
            final txState = txSnapshot.data;
            final hasFunds =
                (txState?.balance ?? 0) > 0 ||
                (txState?.untrustedPendingBalance ?? 0) > 0;
            final color = hasFunds
                ? Theme.of(context).colorScheme.error
                : cautionColor;

            if (shrink) {
              return Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8.0),
                child: IconButton(
                  onPressed: () => onTap(context, walletCtx),
                  icon: Icon(Icons.warning_rounded),
                  style: IconButton.styleFrom(foregroundColor: color),
                  tooltip: 'This wallet has unfinished backups!',
                ),
              );
            }

            return ListTile(
              dense: true,
              contentPadding: EdgeInsets.symmetric(horizontal: 16),
              onTap: () => onTap(context, walletCtx),
              iconColor: color,
              textColor: color,
              leading: Icon(Icons.warning_rounded),
              trailing: Icon(Icons.chevron_right),
              title: Text('This wallet has unfinished backups!'),
            );
          },
        );
      },
    );
  }

  void onTap(BuildContext context, WalletContext walletContext) async {
    await MaybeFullscreenDialog.show(
      context: context,
      child: walletContext.wrap(
        BackupChecklist(
          accessStructure: frostKey.accessStructures()[0],
          showAppBar: true,
        ),
      ),
    );
  }
}
