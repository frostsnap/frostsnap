import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/backup_workflow.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/keygen.dart';
import 'package:frostsnapp/psbt.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet_list_controller.dart';
import 'package:frostsnapp/wallet_receive.dart';
import 'package:frostsnapp/wallet_send.dart';
import 'package:frostsnapp/settings.dart';
import 'package:url_launcher/url_launcher.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class Wallet {
  final SuperWallet superWallet;
  final MasterAppkey masterAppkey;

  Wallet({required this.superWallet, required this.masterAppkey});

  FrostKey? frostKey() {
    return coord.getFrostKey(keyId: keyId());
  }

  KeyId keyId() {
    return api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey);
  }
}

class WalletHome extends StatelessWidget {
  const WalletHome({super.key});

  Widget buildNoWalletBody(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final walletListController = homeCtx.walletListController;
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
                  padding: const EdgeInsets.all(20.0),
                  child: Text(
                    'Let\'s Get Started',
                    style: theme.textTheme.headlineLarge,
                  ),
                ),
                OutlinedButton.icon(
                  onPressed:
                      () => Navigator.push(
                        context,
                        MaterialPageRoute(
                          builder: (context) => homeCtx.wrap(KeyNamePage()),
                        ),
                      ),
                  icon: Icon(Icons.add_circle),
                  label: Text('Create Wallet'),
                ),
                TextButton.icon(
                  onPressed:
                      () => showRecoverWalletsDialog(
                        context,
                        walletListController,
                      ),
                  icon: Icon(Icons.history),
                  label: Text(
                    (walletListController.recoverables.isEmpty)
                        ? 'Recover Wallet'
                        : 'Recover Wallet (${walletListController.recoverables.length})',
                  ),
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
        return walletListController.wallets.isEmpty
            ? buildNoWalletBody(context)
            : walletListController.selected?.tryWrapInWalletContext(
                  context: context,
                  child: TxList(),
                ) ??
                SizedBox();
      },
    );
    final bottomBar = ListenableBuilder(
      listenable: walletListController,
      builder: (context, _) {
        return walletListController.selected?.tryWrapInWalletContext(
              context: context,
              child: WalletBottomBar(),
            ) ??
            BottomAppBar(color: Colors.transparent);
      },
    );

    final mediaSize = MediaQuery.sizeOf(context);
    final isNarrowDisplay = mediaSize.width < 840;
    final drawer = WalletDrawer(
      scaffoldKey: scaffoldKey,
      isRounded: isNarrowDisplay,
    );

    if (mediaSize.width < 840) {
      return Scaffold(
        key: scaffoldKey,
        extendBody: true,
        resizeToAvoidBottomInset: false,
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

class TxItem extends StatelessWidget {
  final Transaction transaction;

  const TxItem({super.key, required this.transaction});

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

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;

    final theme = Theme.of(context);
    final txid = transaction.txid();

    final blockHeight = walletContext.superWallet.height();
    final blockCount = blockHeight + 1;
    final timestamp = transaction.timestamp();

    final confirmations =
        blockCount - (transaction.confirmationTime?.height ?? blockCount);

    final nowDateTime = DateTime.now();
    final dateTime =
        (timestamp != null)
            ? DateTime.fromMillisecondsSinceEpoch(timestamp * 1000)
            : DateTime.now();
    final dateTimeText = humanReadableTimeDifference(nowDateTime, dateTime);

    final isConfirmed = confirmations > 0;
    final isSend = transaction.netValue < 0;

    final iconColor = Color.lerp(
      transaction.netValue > 0
          ? theme.colorScheme.primary
          : theme.colorScheme.error,
      theme.disabledColor,
      confirmations > 0 ? 0.0 : 1.0,
    );
    final Widget icon = Badge(
      alignment: AlignmentDirectional.bottomEnd,
      label: Icon(Icons.hourglass_top_rounded, size: 12.0),
      isLabelVisible: !isConfirmed,
      backgroundColor: Colors.transparent,
      child: Icon(
        transaction.netValue > 0 ? Icons.south_east : Icons.north_east,
        color: iconColor,
      ),
    );

    buildTile(MenuController controller) => ListTile(
      title: Text(
        isConfirmed
            ? (isSend ? 'Sent' : 'Received')
            : (isSend ? 'Sending...' : 'Receiving...'),
        style: theme.textTheme.titleMedium?.copyWith(
          color: isConfirmed ? null : iconColor,
        ),
      ),
      subtitle: Text(
        dateTimeText,
        style: isConfirmed ? null : TextStyle(color: theme.disabledColor),
      ),
      leading: icon,
      trailing: SatoshiText(
        value: transaction.netValue,
        showSign: true,
        style: theme.textTheme.bodyLarge?.copyWith(
          color: isConfirmed ? null : iconColor,
        ),
        disabledColor: isConfirmed ? null : theme.colorScheme.outlineVariant,
      ),
      onLongPress:
          () => controller.isOpen ? controller.close() : controller.open(),
    );

    rebroadcastAction(BuildContext context) {
      walletContext.superWallet.rebroadcast(txid: txid);
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Transaction rebroadcasted')));
    }

    copyAction(BuildContext context) {
      Clipboard.setData(ClipboardData(text: transaction.txid()));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Transaction ID copied to clipboard')),
      );
    }

    final screenWidth = MediaQuery.of(context).size.width;

    return MenuAnchor(
      alignmentOffset: const Offset(32.0, -8.0),
      menuChildren: [
        MenuItemButton(
          onPressed: () => copyAction(context),
          leadingIcon: const Icon(Icons.copy),
          child: SizedBox(
            width: screenWidth * 2 / 3,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              spacing: 4.0,
              children: [
                Text('Copy Transaction ID'),
                Text(
                  txid,
                  softWrap: true,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.titleSmall?.copyWith(
                    color: theme.disabledColor,
                  ),
                ),
              ],
            ),
          ),
        ),
        MenuItemButton(
          onPressed: () async {
            final explorer = getBlockExplorer(
              walletContext.superWallet.network,
            );
            final url = explorer.replace(path: "${explorer.path}tx/$txid");
            await launchUrl(url);
          },
          leadingIcon: SizedBox(
            width: IconTheme.of(context).size ?? 24,
            height: IconTheme.of(context).size ?? 24,
            child: Image.asset('assets/icons/mempool.png'),
          ),
          child: Text('View in mempool.space'),
        ),
        if (transaction.confirmationTime == null)
          MenuItemButton(
            onPressed: () => rebroadcastAction(context),
            leadingIcon: const Icon(Icons.publish),
            child: const Text('Rebroadcast'),
          ),
      ],
      builder: (ctx, controller, _) => buildTile(controller),
    );
  }
}

startRecovery(BuildContext context, RecoverableKey recoverableKey) {
  try {
    coord.startRecovery(keyId: recoverableKey.accessStructureRef.keyId);
  } on FrbAnyhowException catch (e) {
    if (context.mounted) {
      showErrorSnackbarBottom(context, e.anyhow);
    }
  }
}

showRecoverWalletsDialog(
  BuildContext context,
  WalletListController controller,
) {
  final theme = Theme.of(context);

  final appBar = SliverAppBar(
    title: Text('Recover Wallet'),
    titleTextStyle: theme.textTheme.titleMedium,
    centerTitle: true,
    backgroundColor: theme.colorScheme.surfaceContainerLow,
    pinned: true,
    stretch: true,
    forceMaterialTransparency: true,
    automaticallyImplyLeading: false,
    leading: IconButton(
      onPressed: () => Navigator.pop(context),
      icon: Icon(Icons.close),
    ),
  );

  final list = ListenableBuilder(
    listenable: controller,
    builder: (context, _) {
      var mediaQuery = MediaQuery.of(context);
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: 24.0,
              vertical: 8.0,
            ),
            child: Text(
              'Plug in a device to recover from.',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          ...controller.recovering
              .where((key) => key.recoveringAccessIds.isNotEmpty)
              .map((key) {
                final accessId = key.recoveringAccessIds.first;
                final threshold = key.thesholdFor(accessId) ?? 0;
                final obtained = key.devicesFor(accessId)?.length ?? 0;

                return Card.filled(
                  margin: EdgeInsets.symmetric(vertical: 8.0, horizontal: 24.0),
                  child: ListTile(
                    title: Text(key.name),
                    subtitle: Text(
                      'Need to visit ${threshold - obtained} more device(s)',
                    ),
                    trailing: CircularProgressIndicator(
                      value: obtained.toDouble() / threshold.toDouble(),
                    ),
                  ),
                );
              }),
          ...controller.recoverables.map((recoverableKey) {
            final canRecoverNow =
                recoverableKey.sharesObtained >= recoverableKey.threshold;
            return Card.filled(
              margin: const EdgeInsets.symmetric(
                vertical: 8.0,
                horizontal: 24.0,
              ),
              child: ListTile(
                title: Text(recoverableKey.name),
                subtitle: Text(
                  canRecoverNow ? "Recoverable now" : 'Ready to begin recovery',
                ),
                trailing:
                    canRecoverNow
                        ? FilledButton(
                          onPressed:
                              () => startRecovery(context, recoverableKey),
                          child: Text('Recover'),
                        )
                        : OutlinedButton(
                          onPressed:
                              () => startRecovery(context, recoverableKey),
                          child: Text('Begin'),
                        ),
              ),
            );
          }),
          SizedBox(
            height:
                24 + mediaQuery.viewInsets.bottom + mediaQuery.padding.bottom,
          ),
        ],
      );
    },
  );

  final scrollView = CustomScrollView(
    shrinkWrap: true,
    physics: ClampingScrollPhysics(),
    slivers: [
      appBar,
      SliverToBoxAdapter(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: 210),
          child: Center(child: list),
        ),
      ),
    ],
  );

  final mediaSize = MediaQuery.sizeOf(context);
  if (mediaSize.width < 600) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      isDismissible: true,
      showDragHandle: false,
      builder: (context) => scrollView,
    );
  } else {
    showDialog(
      context: context,
      builder: (context) {
        return Dialog(
          backgroundColor: theme.colorScheme.surfaceContainer,
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 560),
            child: scrollView,
          ),
        );
      },
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
          onPressed:
              () => Navigator.push(
                context,
                MaterialPageRoute(
                  builder: (context) => LoadPsbtPage(wallet: walletCtx.wallet),
                ),
              ),
          leadingIcon: Icon(Icons.key),
          child: Text('Sign PSBT'),
        ),
        MenuItemButton(
          onPressed:
              (frostKey == null)
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
          onPressed:
              () => Navigator.push(
                context,
                MaterialPageRoute(
                  builder: (context) => walletCtx.wrap(SettingsPage()),
                ),
              ),
          leadingIcon: Icon(Icons.settings),
          child: Text('Settings'),
        ),
      ],
      builder:
          (_, controller, child) => IconButton(
            onPressed:
                () =>
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
          stream: walletCtx.txStream,
          builder: (context, snapshot) {
            final transactions = snapshot.data?.txs ?? [];
            return SliverList.builder(
              itemCount: transactions.length,
              itemBuilder:
                  (context, index) => TxItem(transaction: transactions[index]),
            );
          },
        ),
        SliverToBoxAdapter(child: SizedBox(height: 88.0)),
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
    WidgetSpan buildTag(
      BuildContext context, {
      required String text,
      Color? backgroundColor,
      Color? foregroundColor,
    }) {
      final theme = Theme.of(context);
      return WidgetSpan(
        alignment: PlaceholderAlignment.middle,
        child: Card.filled(
          color: backgroundColor?.withAlpha(128),
          margin: const EdgeInsets.all(12.0),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4.0, horizontal: 8.0),
            child: Text(
              text,
              style: theme.textTheme.labelSmall?.copyWith(
                color: foregroundColor,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
        ),
      );
    }

    final theme = Theme.of(context);
    return NavigationDrawerDestination(
      icon: SizedBox.shrink(),
      label: Text.rich(
        TextSpan(
          text: item.name,
          children: [
            if (!(item.network?.isMainnet() ?? true))
              buildTag(
                context,
                text: item.network?.name() ?? '',
                backgroundColor: theme.colorScheme.surfaceContainerLowest,
                foregroundColor: theme.colorScheme.error,
              ),
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
          AppBar(
            title: Text('Wallets'),
            titleTextStyle: theme.textTheme.titleMedium,
            primary: false,
            automaticallyImplyLeading: false,
            forceMaterialTransparency: true,
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
            () => Navigator.push(
              context,
              MaterialPageRoute(
                builder: (context) => homeCtx.wrap(KeyNamePage()),
              ),
            ),
            true,
            Icons.add_circle,
            'Create Wallet',
          ),
          (
            () => showRecoverWalletsDialog(context, controller),
            false,
            Icons.update,
            (controller.recoverables.isEmpty)
                ? 'Recover Wallet'
                : 'Recover Wallet (${controller.recoverables.length})',
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
              label:
                  isFilled
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
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          Expanded(
            child: ElevatedButton.icon(
              onPressed:
                  () => Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (context) => walletCtx.wrap(WalletReceivePage()),
                    ),
                  ),
              label: Text('Receive'),
              icon: Icon(Icons.south_east),
              style: ElevatedButton.styleFrom(
                elevation: elevation,
                backgroundColor: ElevationOverlay.applySurfaceTint(
                  theme.colorScheme.surfaceContainer,
                  theme.colorScheme.primary,
                  elevation,
                ),
                foregroundColor: theme.colorScheme.primary,
                iconColor: theme.colorScheme.primary,
              ),
            ),
          ),
          Expanded(
            child: ElevatedButton.icon(
              onPressed: () {
                showBottomSheetOrDialog(
                  context,
                  builder: (context) => walletCtx.wrap(WalletSendPage()),
                );
              },
              label: Text('Send'),
              icon: Icon(Icons.north_east),
              style: ElevatedButton.styleFrom(
                elevation: elevation,
                backgroundColor: ElevationOverlay.applySurfaceTint(
                  theme.colorScheme.surfaceContainer,
                  theme.colorScheme.error,
                  elevation,
                ),
                foregroundColor: theme.colorScheme.error,
                iconColor: theme.colorScheme.error,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(28.0),
                ),
              ),
            ),
          ),
        ],
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
    Key? key,
    required this.txStream,
    required this.atTopNotifier,
    this.scrolledUnderElevation,
    this.expandedHeight = 180.0,
  }) : super(key: key);

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
      var pendingIncomingBalance = 0;
      var avaliableBalance = 0;
      for (final tx in txState.txs) {
        if (tx.confirmationTime == null && tx.netValue > 0) {
          pendingIncomingBalance += tx.netValue;
        } else {
          avaliableBalance += tx.netValue;
        }
      }
      if (avaliableBalance < 0) {
        pendingIncomingBalance += avaliableBalance;
        avaliableBalance = 0;
      }
      setState(() {
        this.pendingIncomingBalance = pendingIncomingBalance;
        this.avaliableBalance = avaliableBalance;
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
      builder:
          (context, atTop, _) => Stack(
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
                  padding: EdgeInsets.symmetric(horizontal: 24.0).copyWith(
                    bottom: atTop ? (widget.expandedHeight / 10) : 20.0,
                  ),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    mainAxisAlignment: MainAxisAlignment.start,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SatoshiText(
                        key: UniqueKey(),
                        value: avaliableBalance,
                        style:
                            atTop
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
    Key? key,
    required this.value,
    this.showSign = false,
    this.hideLeadingWhitespace = false,
    this.letterSpacingReductionFactor = 0.0,
    this.style,
    this.disabledColor,
    this.align = TextAlign.right,
  }) : super(key: key);

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

    final List<TextSpan> spans =
        unformatted.characters.indexed.map((elem) {
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
    final homeCtx = HomeContext.of(context)!;
    final walletCtx = WalletContext.of(context)!;
    final showingDialog = homeCtx.isShowingCreatedWalletDialog;
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

    return SliverToBoxAdapter(
      child: ValueListenableBuilder(
        valueListenable: showingDialog,
        child: streamedBanner,
        builder:
            (context, isShowingDialog, streamedBanner) =>
                isShowingDialog ? SizedBox.shrink() : streamedBanner!,
      ),
    );
  }

  onTap(BuildContext context, WalletContext walletContext) {
    showBottomSheetOrDialog(
      context,
      builder:
          (context) => walletContext.wrap(
            BackupChecklist(
              accessStructure: frostKey.accessStructures()[0],
              showAppBar: true,
            ),
          ),
    );
  }

  bool isBackupDone(BackupRun backupRun) =>
      backupRun.devices.every((elem) => elem.$2 != null);
}
