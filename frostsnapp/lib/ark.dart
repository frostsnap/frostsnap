// Ark boarding — beta, signet only, behind developer mode. See rust/src/api/ark.rs.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/ark.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_send.dart';

/// One Ark wallet per network, opened on first use.
class ArkService {
  /// Set once at startup, before anything asks for a wallet.
  static String? appDir;

  static final Map<BitcoinNetwork, Future<ArkWallet>> _opening = {};
  static final Map<BitcoinNetwork, ArkWallet> _open = {};

  static bool supports(BitcoinNetwork network) =>
      network == BitcoinNetwork.signet;

  static final Map<BitcoinNetwork, ValueNotifier<ArkBalance?>> _balances = {};

  /// The latest Ark balance, shared by the home screen header and the Ark page.
  static ValueNotifier<ArkBalance?> balanceOf(BitcoinNetwork network) =>
      _balances.putIfAbsent(network, () => ValueNotifier(null));

  /// Whether the home screen's on-chain / Ark breakdown is expanded.
  static final detailsOpen = ValueNotifier(false);

  static Future<ArkWallet> open(BitcoinNetwork network) {
    final dir = appDir;
    if (dir == null) return Future.error('Ark: app directory not set');
    return _opening.putIfAbsent(network, () async {
      try {
        final wallet = await ArkWallet.open(appDir: dir, network: network);
        _open[network] = wallet;
        return wallet;
      } catch (_) {
        // Don't cache a failure, so that refresh retries.
        _opening.remove(network);
        rethrow;
      }
    });
  }

  /// The wallet if it is already open. The broadcast path asks this synchronously: a board
  /// address can only have been handed out by a wallet that was opened.
  static ArkWallet? opened(BitcoinNetwork network) => _open[network];
}

class ArkPage extends StatefulWidget {
  final ScrollController? scrollController;
  const ArkPage({super.key, this.scrollController});

  @override
  State<ArkPage> createState() => _ArkPageState();
}

class _ArkPageState extends State<ArkPage> {
  ArkWallet? ark;
  ArkBalance? balance;
  int? minBoard;
  String? serverPubkey;
  String? error;
  bool busy = true;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (ark == null && error == null) _load();
  }

  Future<void> _load() async {
    final network = WalletContext.of(context)!.superWallet.network;
    setState(() => busy = true);
    try {
      final ark = await ArkService.open(network);
      await ark.sync_();
      final balance = await ark.balance();
      final minBoard = await ark.minBoardSat();
      final serverPubkey = await ark.serverPubkey();
      ArkService.balanceOf(network).value = balance;
      if (!mounted) return;
      setState(() {
        this.ark = ark;
        this.balance = balance;
        this.minBoard = minBoard;
        this.serverPubkey = serverPubkey;
        error = null;
      });
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> _moveToArk() async {
    final walletCtx = WalletContext.of(context)!;
    final ark = this.ark!;
    final ArkBoardAddress board;
    try {
      board = await ark.boardAddress();
    } catch (e) {
      if (mounted)
        showErrorSnackbar(context, 'Could not get a board address: $e');
      return;
    }
    if (!mounted) return;
    await showBottomSheetOrDialog(
      context,
      title: Text('Move to Ark'),
      builder: (context, scrollController) => walletCtx.wrap(
        WalletSendPage(
          scrollController: scrollController,
          superWallet: walletCtx.superWallet,
          masterAppkey: walletCtx.masterAppkey,
          initialAddress: board.address.toString(),
        ),
      ),
    );
    if (mounted) await _load();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final ark = this.ark;
    final balance = this.balance;

    final children = <Widget>[
      Card.filled(
        color: theme.colorScheme.tertiaryContainer,
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Text(
            'Beta · signet only. Your devices sign the on-chain payment that '
            'funds the board; the Ark server cosigns before it is broadcast. '
            'The VTXO key is held by this app, not by your devices.',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onTertiaryContainer,
            ),
          ),
        ),
      ),
      ListTile(
        leading: Icon(Icons.hub_outlined),
        title: Text('Ark server · Second (signet)'),
        subtitle: SelectableText(
          [
            arkServerAddress(
                  network: WalletContext.of(context)!.superWallet.network,
                ) ??
                'unsupported network',
            if (serverPubkey != null) 'pubkey $serverPubkey',
          ].join('\n'),
        ),
      ),
      if (error != null)
        ListTile(
          leading: Icon(Icons.error_outline, color: theme.colorScheme.error),
          title: Text('Ark unavailable'),
          subtitle: Text(error!),
          trailing: IconButton(icon: Icon(Icons.refresh), onPressed: _load),
        ),
      if (ark != null) ...[
        ListTile(
          leading: Icon(Icons.account_balance_wallet_outlined),
          title: Text('Ark balance'),
          trailing: SatoshiText(value: balance?.spendableSat ?? 0),
        ),
        if ((balance?.pendingBoardSat ?? 0) > 0)
          ListTile(
            leading: Icon(Icons.hourglass_top_rounded),
            title: Text('Boarding'),
            subtitle: Text('Waiting for the funding transaction to confirm'),
            trailing: SatoshiText(value: balance!.pendingBoardSat),
          ),
        if (minBoard != null)
          ListTile(
            leading: Icon(Icons.info_outline),
            title: Text('Minimum board'),
            trailing: SatoshiText(value: minBoard),
          ),
      ],
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
        child: Row(
          spacing: 8,
          children: [
            OutlinedButton.icon(
              onPressed: busy ? null : _load,
              icon: Icon(Icons.sync),
              label: Text('Refresh'),
            ),
            Expanded(
              child: FilledButton.icon(
                onPressed: busy || ark == null ? null : _moveToArk,
                icon: Icon(Icons.login_rounded),
                label: Text('Move to Ark'),
              ),
            ),
          ],
        ),
      ),
    ];

    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      slivers: [
        if (busy) SliverToBoxAdapter(child: LinearProgressIndicator()),
        SliverPadding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          sliver: SliverList.list(children: children),
        ),
      ],
    );
  }
}

/// Keeps [ArkService.balanceOf] fresh — on wallet changes and every minute, since a board
/// becomes spendable when its funding transaction confirms — and, when the header's breakdown
/// is expanded, shows it: on-chain and Ark side by side, after Noah's balance details.
class ArkBalanceDetails extends StatefulWidget {
  final Stream<TxState> txStream;
  const ArkBalanceDetails({super.key, required this.txStream});

  @override
  State<ArkBalanceDetails> createState() => _ArkBalanceDetailsState();
}

class _ArkBalanceDetailsState extends State<ArkBalanceDetails> {
  StreamSubscription? sub;
  Timer? timer;
  bool refreshing = false;
  TxState? txState;

  @override
  void initState() {
    super.initState();
    sub = widget.txStream.listen((txState) {
      if (mounted) setState(() => this.txState = txState);
      _refresh();
    });
    timer = Timer.periodic(const Duration(minutes: 1), (_) => _refresh());
    WidgetsBinding.instance.addPostFrameCallback((_) => _refresh());
  }

  @override
  void dispose() {
    sub?.cancel();
    timer?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    if (refreshing || !mounted) return;
    final network = WalletContext.of(context)?.superWallet.network;
    if (network == null || !ArkService.supports(network)) return;
    refreshing = true;
    try {
      final ark = await ArkService.open(network);
      await ark.sync_();
      ArkService.balanceOf(network).value = await ark.balance();
    } catch (_) {
      // The Ark page shows the error; the header simply shows no Ark balance.
    } finally {
      refreshing = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: Listenable.merge([
        ArkService.detailsOpen,
        ArkService.balanceOf(walletCtx.superWallet.network),
      ]),
      builder: (context, _) {
        final ark = ArkService.balanceOf(walletCtx.superWallet.network).value;
        if (!ArkService.detailsOpen.value || ark == null) {
          return const SizedBox.shrink();
        }
        final theme = Theme.of(context);
        final muted = theme.textTheme.bodyMedium?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        );
        final bold = theme.textTheme.titleSmall?.copyWith(
          fontWeight: FontWeight.w700,
        );
        Widget row(String label, int sats, {TextStyle? style}) => Padding(
          padding: const EdgeInsets.symmetric(vertical: 2),
          child: Row(
            children: [
              Expanded(child: Text(label, style: style ?? muted)),
              SatoshiText(value: sats, style: style),
            ],
          ),
        );
        final onchain = txState?.balance ?? 0;
        final onchainPending = txState?.untrustedPendingBalance ?? 0;
        return Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
          child: Card.filled(
            color: theme.colorScheme.surfaceContainer,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Balance details',
                    style: theme.textTheme.titleMedium,
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: 12),
                  row('On-chain', onchain + onchainPending, style: bold),
                  Padding(
                    padding: const EdgeInsets.only(left: 16),
                    child: Column(
                      children: [
                        row('Confirmed', onchain),
                        row('Pending incoming', onchainPending),
                      ],
                    ),
                  ),
                  const SizedBox(height: 12),
                  row(
                    'Ark · Second signet (beta)',
                    ark.spendableSat + ark.pendingBoardSat,
                    style: bold,
                  ),
                  Padding(
                    padding: const EdgeInsets.only(left: 16),
                    child: Column(
                      children: [
                        row('Spendable', ark.spendableSat),
                        row('Pending board', ark.pendingBoardSat),
                      ],
                    ),
                  ),
                  const SizedBox(height: 8),
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton.icon(
                      icon: const Icon(Icons.hub_outlined),
                      label: const Text('Open Ark'),
                      onPressed: () => showBottomSheetOrDialog(
                        context,
                        title: const Text('Ark (beta)'),
                        builder: (context, scrollController) => walletCtx.wrap(
                          ArkPage(scrollController: scrollController),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// On-chain and Ark on a line each under the headline total; tapping expands
/// [ArkBalanceDetails].
class ArkBalanceSplit extends StatelessWidget {
  final int onchain;
  final int ark;
  const ArkBalanceSplit({super.key, required this.onchain, required this.ark});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final style = theme.textTheme.bodyMedium?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );
    final iconSize = style?.fontSize;
    Widget line(IconData icon, String label, int sats, {Widget? trailing}) =>
        Row(
          mainAxisAlignment: MainAxisAlignment.end,
          spacing: 6,
          children: [
            Icon(icon, size: iconSize, color: style?.color),
            Text(label, style: style),
            SatoshiText(value: sats, style: style),
            trailing ?? SizedBox(width: iconSize),
          ],
        );
    return ValueListenableBuilder(
      valueListenable: ArkService.detailsOpen,
      builder: (context, open, _) => InkWell(
        borderRadius: const BorderRadius.all(Radius.circular(8)),
        onTap: () => ArkService.detailsOpen.value = !open,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            spacing: 2,
            children: [
              line(Icons.link_rounded, 'On-chain', onchain),
              line(
                Icons.hub_outlined,
                'Ark',
                ark,
                trailing: AnimatedRotation(
                  turns: open ? 0.5 : 0,
                  duration: Durations.short4,
                  child: Icon(
                    Icons.expand_more_rounded,
                    size: iconSize,
                    color: style?.color,
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
