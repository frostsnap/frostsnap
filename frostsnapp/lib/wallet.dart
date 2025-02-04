import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/psbt.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/theme.dart';
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

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final theme = Theme.of(context);

    const elevation = 3.0;
    final txList = TxList();

    return Scaffold(
      extendBody: true,
      resizeToAvoidBottomInset: false,
      body: txList,
      bottomNavigationBar: ClipRect(
        child: BottomAppBar(
          color: Colors.transparent,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: 16,
            children: [
              Expanded(
                child: ElevatedButton.icon(
                  onPressed: () => Navigator.of(context).push(
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
                    final mediaSize = MediaQuery.sizeOf(context);
                    if (mediaSize.width < 600) {
                      showModalBottomSheet(
                        context: context,
                        isScrollControlled: true,
                        useSafeArea: true,
                        isDismissible: true,
                        showDragHandle: false,
                        builder: (context) => walletCtx.wrap(WalletSendPage()),
                      );
                    } else {
                      showDialog(
                        context: context,
                        builder: (context) => Dialog(
                          backgroundColor: theme.colorScheme.surfaceContainer,
                          child: ConstrainedBox(
                          constraints: BoxConstraints(maxWidth: 560),
                            child: walletCtx.wrap(WalletSendPage()),
                          ),
                        ),
                      );
                    }
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
        ),
      ),
    );
  }
}

void copyToClipboard(BuildContext context, String copyText) {
  Clipboard.setData(ClipboardData(text: copyText)).then((_) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Copied to clipboard!')),
      );
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
    _progressFadeController =
        AnimationController(vsync: this, duration: Duration(seconds: 2));
    widget.progressStream.listen((event) {
      if (!context.mounted) return;
      setState(() => progress = event);
    }, onDone: () {
      if (!context.mounted) return;
      // trigger rebuild to start the animation
      setState(() => _progressFadeController.forward());
    });
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
          child: LinearProgressIndicator(
            value: progress,
          ),
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
    final dateTime = (timestamp != null)
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
        confirmations > 0 ? 0.0 : 1.0);
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
            style: theme.textTheme.titleMedium
                ?.copyWith(color: isConfirmed ? null : iconColor),
          ),
          subtitle: Text(
            dateTimeText,
            style: isConfirmed ? null : TextStyle(color: theme.disabledColor),
          ),
          leading: icon,
          trailing: SatoshiText(
            value: transaction.netValue,
            showSign: true,
            style: theme.textTheme.bodyLarge
                ?.copyWith(color: isConfirmed ? null : iconColor),
            disabledColor:
                isConfirmed ? null : theme.colorScheme.outlineVariant,
          ),
          onLongPress: () =>
              controller.isOpen ? controller.close() : controller.open(),
        );

    rebroadcastAction(BuildContext context) {
      walletContext.superWallet.rebroadcast(txid: txid);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Transaction rebroadcasted'),
        ),
      );
    }

    copyAction(BuildContext context) {
      Clipboard.setData(ClipboardData(text: transaction.txid()));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Transaction ID copied to clipboard'),
        ),
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
                  style: theme.textTheme.titleSmall
                      ?.copyWith(color: theme.disabledColor),
                )
              ],
            ),
          ),
        ),
        MenuItemButton(
          onPressed: () async {
            final explorer =
                getBlockExplorer(walletContext.superWallet.network);
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
          )
      ],
      builder: (ctx, controller, _) => buildTile(controller),
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

  @override
  void initState() {
    super.initState();
    scrollController.addListener(() {
      if (!context.mounted) return;
      atTopNotifier.value = scrollController.offset <= 48.0;
    }); // medium: 48.0, large: 88.0
  }

  @override
  void dispose() {
    scrollController.dispose();
    atTopNotifier.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final settingsCtx = SettingsContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);

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
                }),
            appBarMenu,
          ],
        ),
        PinnedHeaderSliver(
          child: UpdatingBalance(
            txStream: WalletContext.of(context)!.txStream,
            atTopNotifier: atTopNotifier,
            scrolledUnderElevation: scrolledUnderElevation,
            expandedHeight: 144.0,
          ),
        ),
        StreamBuilder(
          stream: WalletContext.of(context)!.txStream,
          builder: (context, snapshot) {
            final transactions = snapshot.data?.txs ?? [];
            return SliverList.builder(
              itemCount: transactions.length,
              itemBuilder: (context, index) =>
                  TxItem(transaction: transactions[index]),
            );
          },
        ),
        SliverToBoxAdapter(child: SizedBox(height: 88.0)),
      ],
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
  late final StreamSubscription streamSub;

  @override
  void initState() {
    super.initState();
    streamSub = widget.txStream.listen(onData);
  }

  @override
  void dispose() {
    streamSub.cancel();
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
    final pendingBalanceTextStyle =
        theme.textTheme.bodyLarge?.copyWith(color: theme.disabledColor);

    final scrolledColor = ElevationOverlay.applySurfaceTint(
      theme.colorScheme.surfaceContainer,
      theme.colorScheme.surfaceTint,
      theme.appBarTheme.elevation ?? widget.scrolledUnderElevation ?? 3.0,
    );

    const duration = Durations.extralong4;
    const curve = Curves.easeInOutCubicEmphasized;

    final stack = ValueListenableBuilder(
      valueListenable: widget.atTopNotifier,
      builder: (context, atTop, _) => Stack(children: [
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
            padding: EdgeInsets.symmetric(horizontal: 24.0)
                .copyWith(bottom: atTop ? (widget.expandedHeight / 10) : 20.0),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.start,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SatoshiText(
                  key: UniqueKey(),
                  value: avaliableBalance,
                  style:
                      atTop ? balanceTextStyle : theme.textTheme.headlineSmall,
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
      ]),
    );

    return SizedBox(
      height: widget.expandedHeight,
      child: stack,
    );
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

  const SatoshiText.withSign({
    Key? key,
    required int value,
  }) : this(key: key, value: value, showSign: true);

  @override
  Widget build(BuildContext context) {
    final baseStyle = DefaultTextStyle.of(context).style.merge(style).copyWith(
      fontFamily: monospaceTextStyle.fontFamily,
      fontFeatures: [
        FontFeature.slashedZero(),
        FontFeature.tabularFigures(),
      ],
    );

    // We reduce the line spacing by the percentage from the fontSize (as per design specs).
    const defaultWordSpacingFactor = 0.36; // 0.32

    final baseLetterSpacing = (baseStyle.letterSpacing ?? 0.0) -
        (baseStyle.fontSize ?? 0.0) * letterSpacingReductionFactor;
    final wordSpacing = (baseStyle.letterSpacing ?? 0.0) -
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
            text: ' ', style: TextStyle(letterSpacing: wordSpacing));
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
