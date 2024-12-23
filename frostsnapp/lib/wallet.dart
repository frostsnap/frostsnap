import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/wallet_receive.dart';
import 'package:frostsnapp/wallet_send.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:url_launcher/url_launcher.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class WalletContext extends InheritedWidget {
  final Wallet wallet;
  final MasterAppkey masterAppkey;
  final String walletName;
  late final KeyId keyId;
  late final Stream<TxState> txStream;
  // We have a contextual Stream of syncing events (each syncing event is
  // represented as a Stream<double> where the double is the progress).
  final StreamController<Stream<double>> syncs = StreamController.broadcast();

  WalletContext({
    super.key,
    required this.wallet,
    required this.masterAppkey,
    required this.walletName,
    required Widget child,
  }) : super(child: child) {
    keyId = api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey);
    txStream =
        wallet.subTxState(masterAppkey: masterAppkey).toBehaviorSubject();
  }

  WalletContext.withStream({
    super.key,
    required this.wallet,
    required this.masterAppkey,
    required this.walletName,
    required this.txStream,
    required Widget child,
  }) : super(child: child) {
    keyId = api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey);
  }

  static WalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<WalletContext>();
  }

  WalletContext copyWith(Widget child) => WalletContext.withStream(
        wallet: wallet,
        masterAppkey: masterAppkey,
        walletName: walletName,
        txStream: txStream,
        child: child,
      );

  Stream<bool> syncStartStopStream() {
    return syncs.stream.asyncExpand((syncStream) async* {
      yield true;
      try {
        // wait for the sync to finish
        await syncStream.toList();
      } catch (e) {
        // do nothing
      }

      yield false;
    });
  }

  @override
  bool updateShouldNotify(WalletContext oldWidget) {
    // never updates
    return false;
  }
}

class WalletPage extends StatelessWidget {
  final Wallet wallet;
  final MasterAppkey masterAppkey;
  final String walletName;

  const WalletPage(
      {super.key,
      required this.wallet,
      required this.masterAppkey,
      required this.walletName});

  @override
  Widget build(BuildContext context) {
    return WalletContext(
        wallet: wallet,
        masterAppkey: masterAppkey,
        walletName: walletName,
        child: WalletHome());
  }
}

class WalletHome extends StatelessWidget {
  const WalletHome({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final theme = Theme.of(context);

    return Scaffold(
      extendBody: true,
      backgroundColor: theme.colorScheme.surfaceContainer,
      appBar: FsAppBar(title: Text(walletCtx.walletName)),
      body: TxList(),
      resizeToAvoidBottomInset: true,
      floatingActionButtonLocation: FloatingActionButtonLocation.centerFloat,
      floatingActionButton: Padding(
        padding: EdgeInsets.symmetric(horizontal: 20.0),
        child: Row(
          spacing: 10.0,
          children: [
            Expanded(
              child: FloatingActionButton.extended(
                heroTag: null,
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (context) =>
                        walletCtx.copyWith(WalletReceivePage()),
                  ),
                ),
                label: Text("Request"),
              ),
            ),
            Expanded(
              child: FloatingActionButton.extended(
                heroTag: null,
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (context) => walletCtx.copyWith(WalletSendPage()),
                  ),
                ),
                label: Text("Pay"),
              ),
            ),
          ],
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
      setState(() {
        progress = event;
      });
    }, onDone: () {
      // trigger rebuild to start the animation
      setState(() {
        _progressFadeController.forward();
      });
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
            //backgroundColor: backgroundSecondaryColor,
            //valueColor: AlwaysStoppedAnimation<Color>(textPrimaryColor),
          ),
        ),
      ),
    );
  }
}

class TxItem extends StatelessWidget {
  final Transaction transaction;
  static const Map<int, String> monthMap = {
    1: 'Jan',
    2: 'Feb',
    3: 'Mar',
    4: 'Apr',
    5: 'May',
    6: 'Jun',
    7: 'Jul',
    8: 'Aug',
    9: 'Sep',
    10: 'Oct',
    11: 'Nov',
    12: 'Dec',
  };

  const TxItem({super.key, required this.transaction});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;

    final theme = Theme.of(context);
    final txid = transaction.txid();
    final txidText =
        '${txid.substring(0, 6)}...${txid.substring(txid.length - 6, txid.length)}';

    final blockHeight = walletContext.wallet.height();
    final blockCount = blockHeight + 1;
    final timestamp = transaction.timestamp();

    final confirmations =
        blockCount - (transaction.confirmationTime?.height ?? blockCount);

    final dateTime = (timestamp != null)
        ? DateTime.fromMillisecondsSinceEpoch(timestamp * 1000)
        : DateTime.now();
    final dayText = dateTime.day.toString();
    final monthText = monthMap[dateTime.month]!;
    final yearText = dateTime.year.toString();
    final hourText = dateTime.hour.toString().padLeft(2, '0');
    final minuteText = dateTime.minute.toString().padLeft(2, '0');
    final dateText = '$monthText $dayText, $yearText';
    final timeText = (timestamp != null) ? '$hourText:$minuteText' : '??:??';

    final Widget icon = Icon(
      transaction.netValue > 0 ? Icons.south_east : Icons.north_east,
      color: (confirmations == 0)
          ? Colors.white38
          : transaction.netValue > 0
              ? theme.colorScheme.primary
              : theme.colorScheme.error,
    );

    final tile = Padding(
        padding: EdgeInsets.symmetric(vertical: 4.0, horizontal: 24.0),
        child: Row(
          mainAxisSize: MainAxisSize.max,
          spacing: 8.0,
          children: [
            icon,
            Expanded(
              child: Padding(
                padding: EdgeInsets.symmetric(vertical: 12.0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  mainAxisSize: MainAxisSize.max,
                  spacing: 4.0,
                  children: [
                    Text(
                      dateText,
                      softWrap: false,
                      overflow: TextOverflow.fade,
                      style: theme.textTheme.titleMedium,
                    ),
                    Text(
                      timeText,
                      softWrap: false,
                      overflow: TextOverflow.fade,
                      style: theme.textTheme.bodyMedium,
                    ),
                  ],
                ),
              ),
            ),
            Expanded(
              child: Padding(
                padding: EdgeInsets.symmetric(vertical: 12.0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  mainAxisSize: MainAxisSize.max,
                  spacing: 4.0,
                  children: [
                    SatoshiText(
                        value: transaction.netValue,
                        showSign: true,
                        style: theme.textTheme.titleMedium),
                    Text(
                      txidText,
                      softWrap: false,
                      overflow: TextOverflow.ellipsis,
                      style: GoogleFonts.sourceCodePro(
                        textStyle: theme.textTheme.bodyMedium,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ));

    rebroadcastAction(BuildContext context) {
      walletContext.wallet.rebroadcast(txid: txid);
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
                  style: GoogleFonts.sourceCodePro(
                      textStyle: theme.textTheme.titleSmall
                          ?.copyWith(color: Colors.white38)),
                )
              ],
            ),
          ),
        ),
        MenuItemButton(
          onPressed: () async {
            final url = Uri.parse("https://mempool.space/tx/$txid");
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
      builder: (_, MenuController controller, Widget? child) {
        return InkWell(
          borderRadius: BorderRadius.circular(16.0),
          onLongPress: () {
            if (controller.isOpen) {
              controller.close();
            } else {
              controller.open();
            }
          },
          child: tile,
        );
      },
    );
  }
}

class TxList extends StatelessWidget {
  final ScrollController? scrollController;

  const TxList({super.key, this.scrollController});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;
    final scrollController = this.scrollController ?? ScrollController();

    return CustomScrollView(
      controller: scrollController,
      slivers: <Widget>[
        SliverToBoxAdapter(
          child: UpdatingBalance(
              txStream: walletContext.txStream,
              scrollController: scrollController),
        ),
        SliverSafeArea(
          sliver: StreamBuilder(
            stream: walletContext.txStream,
            builder: (context, snapshot) {
              final transactions = snapshot.data?.txs ?? [];
              return SliverList.builder(
                itemCount: transactions.length,
                itemBuilder: (context, index) =>
                    TxItem(transaction: transactions[index]),
              );
            },
          ),
        ),
        SliverToBoxAdapter(child: SizedBox(height: 80)),
      ],
    );
  }
}

class UpdatingBalance extends StatefulWidget {
  final ScrollController? scrollController;
  final Stream<TxState> txStream;

  const UpdatingBalance(
      {Key? key, required this.txStream, this.scrollController})
      : super(key: key);

  @override
  State<UpdatingBalance> createState() => _UpdatingBalanceState();
}

class _UpdatingBalanceState extends State<UpdatingBalance> {
  int pendingIncomingBalance = 0;
  int avaliableBalance = 0;
  bool scrollPosAtTop = true;
  double opacity = 1.0;

  @override
  void initState() {
    super.initState();

    widget.txStream.listen((txState) {
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
    });

    widget.scrollController?.addListener(() {
      if (widget.scrollController == null) return;

      final controller = widget.scrollController!;
      if (scrollPosAtTop) {
        if (controller.offset != 0.0) {
          setState(() => scrollPosAtTop = false);
        }
      } else {
        if (controller.offset == 0.0) {
          setState(() => scrollPosAtTop = true);
        }
      }

      const maxOpacityOffset = 32.0;
      if (controller.offset <= maxOpacityOffset) {
        setState(() {
          opacity = (maxOpacityOffset - controller.offset) / maxOpacityOffset;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final balanceTextStyle = DefaultTextStyle.of(context)
        .style
        .copyWith(fontSize: 36.0, fontWeight: FontWeight.w600);
    final padding = EdgeInsets.all(24.0).copyWith(top: 16.0);

    final backgroundColor = ElevationOverlay.applySurfaceTint(
      scrollPosAtTop
          ? theme.colorScheme.surface
          : theme.colorScheme.surfaceContainer,
      theme.colorScheme.surfaceTint,
      scrollPosAtTop ? 0 : theme.appBarTheme.elevation ?? 3.0,
    );

    final separatorContainer = Container(
      width: double.infinity,
      height: 16.0,
      color: backgroundColor,
      foregroundDecoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainer,
        borderRadius: BorderRadius.only(
          topLeft: Radius.circular(16),
          topRight: Radius.circular(16),
        ),
      ),
    );

    final textColumn = Column(
      spacing: 8.0,
      children: [
        SatoshiText(
          value: avaliableBalance,
          style: balanceTextStyle,
          letterSpacingReductionFactor: 0.02,
        ),
        if (pendingIncomingBalance > 0)
          Row(
            mainAxisSize: MainAxisSize.min,
            spacing: 4.0,
            children: [
              Icon(Icons.hourglass_top, size: 12.0),
              SatoshiText(
                value: pendingIncomingBalance,
                showSign: true,
              ),
            ],
          ),
      ],
    );

    return Column(children: [
      Container(
        padding: padding,
        color: backgroundColor,
        child: Row(
          mainAxisSize: MainAxisSize.max,
          children: [
            Expanded(
              child: Opacity(opacity: opacity, child: textColumn),
            )
          ],
        ),
      ),
      separatorContainer,
    ]);
  }
}

class SatoshiText extends StatelessWidget {
  final int value;
  final bool showSign;
  final double opacityChangeFactor;
  final double letterSpacingReductionFactor;
  final TextStyle? style;

  const SatoshiText({
    Key? key,
    required this.value,
    this.showSign = false,
    this.opacityChangeFactor = 0.5,
    this.letterSpacingReductionFactor = 0.0,
    this.style,
  }) : super(key: key);

  const SatoshiText.withSign({
    Key? key,
    required int value,
  }) : this(key: key, value: value, showSign: true);

  @override
  Widget build(BuildContext context) {
    final baseStyle = GoogleFonts.inter(
        textStyle: style ?? DefaultTextStyle.of(context).style);
    // We reduce the line spacing by the percentage from the fontSize (as per design specs).
    final baseLetterSpacing = (baseStyle.letterSpacing ?? 0.0) -
        (baseStyle.fontSize ?? 0.0) * letterSpacingReductionFactor;

    final activeStyle = baseStyle.copyWith(letterSpacing: baseLetterSpacing);
    final inactiveStyle = baseStyle.copyWith(
      letterSpacing: baseLetterSpacing,
      // Reduce text opacity by `opacityChangeFactor` initially.
      color: baseStyle.color!.withAlpha(
          Color.getAlphaFromOpacity(baseStyle.color!.a * opacityChangeFactor)),
    );

    // Convert to BTC string with 8 decimal places
    String btcString = (value / 100000000.0).toStringAsFixed(8);
    // Split the string into two parts, removing - sign: before and after the decimal
    final parts = btcString.replaceFirst(r'-', '').split('.');
    // Format the fractional part into segments
    final String fractionalPart =
        "${parts[1].substring(0, 2)} ${parts[1].substring(2, 5)} ${parts[1].substring(5)}";
    // Combine the whole number part with the formatted fractional part
    btcString = '${parts[0]}.$fractionalPart \u20BF';
    // Add sign if required.
    if (showSign || !showSign && value.isNegative) {
      btcString = value.isNegative ? '- $btcString' : '+ $btcString';
    }

    var activeIndex = btcString.indexOf(RegExp(r'[1-9]'));
    if (activeIndex == -1) activeIndex = btcString.length - 1;
    final inactiveString = btcString.substring(0, activeIndex);
    final activeString = btcString.substring(activeIndex);

    return Text.rich(
      TextSpan(children: <TextSpan>[
        TextSpan(text: inactiveString, style: inactiveStyle),
        TextSpan(text: activeString, style: activeStyle),
      ]),
      textAlign: TextAlign.right,
      softWrap: false,
      overflow: TextOverflow.fade,
    );
  }
}
