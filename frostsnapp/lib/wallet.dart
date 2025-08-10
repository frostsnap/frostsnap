import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/svg.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_settings.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/keygen.dart';
import 'package:frostsnap/psbt.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_manager.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_create.dart';
import 'package:frostsnap/wallet_list_controller.dart';
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
  const WalletHome({super.key});

  Widget buildNoWalletBody(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    return CustomScrollView(
      slivers: [
        SliverAppBar(pinned: true),
        SliverFillRemaining(
          hasScrollBody: false,
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              spacing: 20.0,
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10.0),
                  child: SvgPicture.asset(
                    'assets/frostsnap-logo.svg',
                    fit: BoxFit.fitWidth,
                    height: 100,
                    colorFilter: ColorFilter.mode(
                      theme.colorScheme.primary,
                      BlendMode.srcATop,
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.all(10.0),
                  child: Text(
                    'Let\'s Get Started',
                    style: theme.textTheme.headlineLarge,
                  ),
                ),
                OutlinedButton.icon(
                  onPressed: () async {
                    final asRef =
                        await MaybeFullscreenDialog.show<AccessStructureRef>(
                          context: context,
                          barrierDismissible: false,
                          child: WalletCreatePage(),
                        );
                    if (context.mounted && asRef != null) {
                      homeCtx.openNewlyCreatedWallet(asRef.keyId);
                      showWalletCreatedDialog(context, asRef);
                    }
                  },
                  icon: Icon(Icons.add_circle),
                  label: Text('Create Wallet'),
                ),
                TextButton.icon(
                  onPressed: () async {
                    final restorationId = await startWalletRecoveryFlowDialog(
                      context,
                    );
                    if (restorationId != null) {
                      homeCtx.walletListController.selectRecoveringWallet(
                        restorationId,
                      );
                    }
                  },
                  icon: Icon(Icons.history),
                  label: Text('Restore Wallet'),
                ),
                SizedBox(height: 100.0),
              ],
            ),
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final homeCtx = HomeContext.of(context)!;
    final walletListController = homeCtx.walletListController;
    final scaffoldKey = homeCtx.scaffoldKey;

    final body = ListenableBuilder(
      listenable: walletListController,
      builder: (context, _) {
        if (!walletListController.gotInitialData) {
          return Center(child: CircularProgressIndicator());
        }

        final selected = walletListController.selected;
        if (selected == null) {
          return buildNoWalletBody(context);
        }

        return switch (selected) {
          WalletItemKey item => item.tryWrapInWalletContext(
            context: context,
            child: TxList(key: Key(item.frostKey.keyId().toHex())),
          ),
          WalletItemRestoration item => WalletRecoveryPage(
            key: Key(item.restoringKey.restorationId.toHex()),
            restoringKey: item.restoringKey,
            onWalletRecovered: (accessStructureRef) {
              walletListController.selectWallet(accessStructureRef.keyId);
            },
          ),
        };
      },
    );
    final bottomBar = ListenableBuilder(
      listenable: walletListController,
      builder: (context, _) {
        return switch (walletListController.selected) {
          WalletItemKey item => item.tryWrapInWalletContext(
            context: context,
            child: WalletBottomBar(),
          ),
          _ => BottomAppBar(color: Colors.transparent),
        };
      },
    );

    final mediaSize = MediaQuery.sizeOf(context);
    final isNarrowDisplay = mediaSize.width < 840;
    final drawer = WalletDrawer(
      scaffoldKey: scaffoldKey,
      isRounded: isNarrowDisplay,
    );
    if (isNarrowDisplay) {
      return Scaffold(
        key: scaffoldKey,
        extendBody: true,
        resizeToAvoidBottomInset: true,
        drawer: drawer,
        body: body,
        bottomNavigationBar: bottomBar,
      );
    } else {
      return Row(
        children: [
          drawer,
          Flexible(
            child: Scaffold(
              key: scaffoldKey,
              extendBody: true,
              resizeToAvoidBottomInset: false,
              body: body,
              bottomNavigationBar: bottomBar,
            ),
          ),
        ],
      );
    }
  }
}

void copyToClipboard(BuildContext context, String copyText) {
  Clipboard.setData(ClipboardData(text: copyText)).then((_) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Copied to clipboard!')));
    }
  });
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
    final settingsCtx = SettingsContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);

    // TODO: This is a hack to scroll to top everytime we switch wallets.
    // There are better ways to do this but requires more involved changes.
    if (prevKey == null || !keyIdEquals(walletCtx.keyId, prevKey!)) {
      prevKey = walletCtx.keyId;
      if (scrollController.hasClients) scrollController.jumpTo(0.0);
    }

    const scrolledUnderElevation = 1.0;

    final appBarMenu = MenuAnchor(
      menuChildren: [
        MenuItemButton(
          onPressed: () => Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => LoadPsbtPage(wallet: walletCtx.wallet),
            ),
          ),
          leadingIcon: Icon(Icons.key),
          child: Text('Sign PSBT'),
        ),
        MenuItemButton(
          onPressed: (frostKey == null)
              ? null
              : () => Navigator.push(
                  context,
                  MaterialPageRoute(
                    builder: (context) => SignMessagePage(frostKey: frostKey),
                  ),
                ),
          leadingIcon: Icon(Icons.key),
          child: Text('Sign Message'),
        ),
        Divider(),
        MenuItemButton(
          onPressed: () => Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => walletCtx.wrap(SettingsPage()),
            ),
          ),
          leadingIcon: Icon(Icons.settings),
          child: Text('Settings'),
        ),
      ],
      builder: (_, controller, child) => IconButton(
        onPressed: () =>
            controller.isOpen ? controller.close() : controller.open(),
        icon: Icon(Icons.more_vert),
      ),
    );

    return CustomScrollView(
      controller: scrollController,
      physics: ClampingScrollPhysics(),
      slivers: <Widget>[
        SliverAppBar.medium(
          pinned: true,
          title: Text(frostKey?.keyName() ?? 'Unknown'),
          scrolledUnderElevation: scrolledUnderElevation,
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
            appBarMenu,
          ],
        ),
        if (frostKey != null) BackupWarningBanner(frostKey: frostKey),
        PinnedHeaderSliver(
          child: UpdatingBalance(
            txStream: walletCtx.txStream,
            atTopNotifier: atTopNotifier,
            scrolledUnderElevation: scrolledUnderElevation,
            expandedHeight: 144.0,
          ),
        ),
        StreamBuilder(
          stream: MergeStream([
            walletCtx.signingSessionSignals,
            // Also rebuild on canonical tx list changes since `unbroadcastedTxs` excludes from the
            // canonical tx list.
            walletCtx.txStream.map((_) => {}),
          ]),
          builder: (context, snapshot) {
            final chainTipHeight = walletCtx.wallet.superWallet.height();
            final now = DateTime.now();
            final txToBroadcastTiles = coord
                .unbroadcastedTxs(
                  superWallet: walletCtx.wallet.superWallet,
                  keyId: walletCtx.keyId,
                )
                .map((tx) {
                  final txDetails = TxDetailsModel(
                    tx: tx.tx,
                    chainTipHeight: chainTipHeight,
                    now: now,
                  );
                  return TxSentOrReceivedTile(
                    onTap: () => showBottomSheetOrDialog(
                      context,
                      title: Text('Transaction Details'),
                      builder: (context, scrollController) => walletCtx.wrap(
                        TxDetailsPage.needsBroadcast(
                          scrollController: scrollController,
                          txStates: walletCtx.txStream,
                          txDetails: txDetails,
                          finishedSigningSessionId: tx.sessionId,
                        ),
                      ),
                    ),
                    txDetails: txDetails,
                  );
                });
            final txToSignTiles = coord
                .activeSigningSessions(keyId: walletCtx.keyId)
                .map<(Transaction, SigningState)?>((session) {
                  final Transaction? tx = switch (session.details()) {
                    SigningDetails_Transaction(:final transaction) =>
                      transaction,
                    _ => null,
                  };
                  if (tx == null) return null;
                  return (tx, session.state());
                })
                .nonNulls
                .map((state) {
                  final (tx, signingState) = state;
                  final txDetails = TxDetailsModel(
                    tx: tx,
                    chainTipHeight: chainTipHeight,
                    now: now,
                  );
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
                        ),
                      ),
                    ),
                    txDetails: txDetails,
                    signingState: signingState,
                  );
                });
            return SliverVisibility(
              visible:
                  txToSignTiles.isNotEmpty || txToBroadcastTiles.isNotEmpty,
              sliver: SliverList.list(
                children: [...txToBroadcastTiles, ...txToSignTiles],
              ),
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
    return NavigationDrawerDestination(
      icon: item.icon ?? SizedBox.shrink(),
      label: Text.rich(
        TextSpan(
          text: item.name,
          children: [
            if (!(item.network?.isMainnet() ?? true))
              buildTag(context, text: item.network?.name() ?? ''),
          ],
        ),
        overflow: TextOverflow.fade,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final controller = homeCtx.walletListController;
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        List<Widget> children = [
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: 10.0,
              vertical: 5.0,
            ),
            child: SvgPicture.asset(
              'assets/frostsnap-logo.svg',
              // width: logoWidth,
              fit: BoxFit.fitWidth,
              height: 100,
              colorFilter: ColorFilter.mode(
                theme.colorScheme.primary,
                BlendMode.srcATop,
              ),
            ),
          ),
        ];
        if (controller.wallets.isEmpty) {
          children.add(
            NavigationDrawerDestination(
              enabled: false,
              icon: SizedBox(),
              label: Text('Let\'s Get Started'),
            ),
          );
        } else {
          children.addAll([
            ...controller.wallets.map(
              (item) => buildWalletDestination(context, item),
            ),
            NavigationDrawerDestination(
              icon: SizedBox.shrink(),
              //label: SizedBox.shrink(),
              label: SizedBox(width: 224, child: Divider()),
              enabled: false,
            ),
          ]);
        }

        final List<(void Function(), bool, IconData, String)>
        actionableDestinations = [
          (
            () async {
              final asRef =
                  await MaybeFullscreenDialog.show<AccessStructureRef>(
                    context: context,
                    barrierDismissible: false,
                    child: WalletCreatePage(),
                  );
              if (context.mounted && asRef != null) {
                homeCtx.openNewlyCreatedWallet(asRef.keyId);
                showWalletCreatedDialog(context, asRef);
              }
            },
            true,
            Icons.add_circle,
            'Create Wallet',
          ),
          (
            () async {
              final restorationId = await startWalletRecoveryFlowDialog(
                context,
              );
              if (restorationId != null) {
                controller.selectRecoveringWallet(restorationId);
              }
            },
            false,
            Icons.update,
            'Restore Wallet',
          ),
          (
            () => Navigator.push(
              context,
              MaterialPageRoute(builder: (context) => DeviceSettingsPage()),
            ),
            false,
            Icons.devices,
            'Devices',
          ),
          (
            () => Navigator.push(
              context,
              MaterialPageRoute(builder: (context) => SettingsPage()),
            ),
            false,
            Icons.settings,
            'Settings',
          ),
        ];
        children.addAll([
          ...actionableDestinations.map((elem) {
            final (onPressed, isFilled, iconData, textData) = elem;
            final label = Text(textData);
            final icon = Icon(iconData);
            return NavigationDrawerDestination(
              enabled: false,
              icon: SizedBox.shrink(),
              label: isFilled
                  ? FilledButton.icon(
                      onPressed: onPressed,
                      icon: icon,
                      label: label,
                    )
                  : TextButton.icon(
                      onPressed: onPressed,
                      icon: icon,
                      label: label,
                    ),
            );
          }),
        ]);

        final drawer = NavigationDrawer(
          onDestinationSelected: (index) {
            controller.selectedIndex = index;
            scaffoldKey.currentState?.closeDrawer();
          },
          selectedIndex: controller.selectedIndex,
          children: children,
        );

        return isRounded
            ? drawer
            : Container(
                color: theme.colorScheme.surfaceContainerLow,
                child: drawer,
              );
      },
    );
  }
}

class WalletBottomBar extends StatelessWidget {
  const WalletBottomBar({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) {
      return SizedBox();
    }
    final theme = Theme.of(context);
    const elevation = 3.0;
    return BottomAppBar(
      color: Colors.transparent,
      child: Align(
        alignment: AlignmentDirectional.center,
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: 560),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: 16,
            children: [
              Expanded(
                child: ElevatedButton.icon(
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
                  style: ElevatedButton.styleFrom(
                    elevation: elevation,
                    backgroundColor: theme.colorScheme.primaryContainer,
                    foregroundColor: theme.colorScheme.onPrimaryContainer,
                  ),
                ),
              ),
              Expanded(
                child: ElevatedButton.icon(
                  onPressed: () => showBottomSheetOrDialog(
                    context,
                    title: Text('Send'),
                    builder: (context, scrollController) => walletCtx.wrap(
                      WalletSendPage(scrollController: scrollController),
                    ),
                  ),
                  label: Text('Send'),
                  icon: Icon(Icons.north_east),
                  style: ElevatedButton.styleFrom(
                    elevation: elevation,
                    backgroundColor: theme.colorScheme.primaryContainer,
                    foregroundColor: theme.colorScheme.onPrimaryContainer,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class UpdatingBalance extends StatefulWidget {
  final ValueNotifier<bool> atTopNotifier;
  final Stream<TxState> txStream;
  final double? scrolledUnderElevation;
  final double expandedHeight;

  const UpdatingBalance({
    super.key,
    required this.txStream,
    required this.atTopNotifier,
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

  const BackupWarningBanner({required this.frostKey, super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final backupStream = walletCtx.backupStream;
    final theme = Theme.of(context);

    final banner = ListTile(
      dense: true,
      onTap: () => onTap(context, walletCtx),
      tileColor: theme.colorScheme.errorContainer,
      iconColor: theme.colorScheme.onErrorContainer,
      textColor: theme.colorScheme.onErrorContainer,
      leading: Icon(Icons.warning_rounded),
      trailing: Icon(Icons.chevron_right),
      title: Text('This wallet has unfinished backups!'),
    );
    final streamedBanner = StreamBuilder<BackupRun>(
      stream: backupStream,
      builder: (context, snapshot) {
        final backupRun = snapshot.data;
        final hideBanner = backupRun == null || isBackupDone(backupRun);
        return hideBanner ? SizedBox.shrink() : banner;
      },
    );

    return SliverToBoxAdapter(child: streamedBanner);
  }

  onTap(BuildContext context, WalletContext walletContext) {
    showBottomSheetOrDialog(
      context,
      title: Text('Backup Checklist'),
      builder: (context, scrollController) => walletContext.wrap(
        BackupChecklist(
          scrollController: scrollController,
          accessStructure: frostKey.accessStructures()[0],
          showAppBar: false,
        ),
      ),
    );
  }

  bool isBackupDone(BackupRun backupRun) =>
      backupRun.devices.every((elem) => elem.$2 != null);
}
