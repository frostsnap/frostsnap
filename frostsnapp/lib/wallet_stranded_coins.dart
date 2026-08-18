import 'package:frostsnap/src/rust/api/broadcast.dart';
import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/send.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_send_feerate_picker.dart';
import 'package:frostsnap/wallet_tx_details.dart';

/// The feerate the nudge judges a coin's rescue against. Nothing is spent at this rate — it is the
/// bar for mentioning a coin at all, chosen so we only raise the alarm about a coin whose rescue
/// would pay for itself at an ordinary feerate. What a rescue actually moves is decided at the rate
/// the user picks in the fee picker, which can be higher or lower than this.
const nudgeFeerate = 10.0;

/// Nudges the user to consolidate coins sitting past the recovery gap.
///
/// A standard restore crawls addresses and stalls at the first unused stretch wider than its
/// gap limit, so a coin past such a stretch would be missed. Any outgoing transaction
/// consolidates those coins automatically (`plan_send` force-selects them); this banner exists
/// so the user knows one is worth making.
class StrandedCoinsBanner extends StatelessWidget {
  const StrandedCoinsBanner({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;

    return StreamBuilder<TxState>(
      stream: walletCtx.txStream,
      builder: (context, snapshot) {
        var count = 0;
        var sats = 0;
        if (snapshot.hasData) {
          final (strandedCount, strandedSats) = walletCtx.superWallet
              .gapStrandedValue(
                masterAppkey: walletCtx.masterAppkey,
                feerate: nudgeFeerate,
              );
          count = strandedCount;
          sats = strandedSats;
        }

        final card = count == 0
            ? SizedBox(width: double.infinity)
            : Card(
                margin: EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                shape: cardShape(context),
                color: tintSurfaceContainer(context, tint: cautionColor),
                clipBehavior: Clip.hardEdge,
                child: ListTile(
                  onTap: () => showBottomSheetOrDialog(
                    context,
                    title: Text('Consolidate'),
                    builder: (context, scrollController) => walletCtx.wrap(
                      ConsolidatePage(
                        scrollController: scrollController,
                        planner: (feerate) =>
                            walletCtx.superWallet.planConsolidate(
                              masterAppkey: walletCtx.masterAppkey,
                              outpoints: walletCtx.superWallet
                                  .gapStrandedOutpoints(
                                    masterAppkey: walletCtx.masterAppkey,
                                    feerate: feerate,
                                  ),
                              feerate: feerate,
                            ),
                      ),
                    ),
                  ),
                  leading: Icon(Icons.merge_rounded, color: cautionColor),
                  trailing: Icon(Icons.chevron_right),
                  title: Text(
                    count == 1
                        ? 'A coin could be missed by a future restore'
                        : '$count coins could be missed by a future restore',
                  ),
                  subtitle: Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Consolidate now, or let your next payment do it automatically',
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                      SatoshiText(
                        value: sats,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
              );

        return AnimatedSize(
          duration: Durations.medium4,
          curve: Curves.easeInOutCubicEmphasized,
          alignment: Alignment.topCenter,
          child: card,
        );
      },
    );
  }
}

/// Reviews and signs a consolidation: a fixed set of the wallet's own coins in, one change
/// output back. The only degree of freedom is the feerate, so this is the send flow's signer
/// step with a consolidation summary above it — deliberately the same surface, since it is the
/// same act. The destination is not shown: it is change, allocated at commit like all change,
/// and does not exist until then.
class ConsolidatePage extends StatefulWidget {
  final ScrollController? scrollController;

  /// Builds the plan for a feerate. The page doesn't know how the input set was chosen, so a
  /// whole-wallet consolidation reuses it with a different planner.
  final SendPlan Function(double feerate) planner;

  /// An optional line above the summary, for entry points that need to say something the rows
  /// cannot (whole-wallet consolidation links every coin on-chain).
  final String? description;

  const ConsolidatePage({
    super.key,
    this.scrollController,
    required this.planner,
    this.description,
  });

  @override
  State<ConsolidatePage> createState() => _ConsolidatePageState();
}

class _ConsolidatePageState extends State<ConsolidatePage> {
  /// Borrowed for the two things the send flow already knows how to do: hold the feerate its
  /// picker sets, and track which devices will sign.
  BuildTxState? state;
  UnitBroadcastSubscription? sub;
  StreamSubscription<void>? stateSub;
  SendPlan? plan;
  String? planError;
  bool estimateRunning = false;
  late final ScrollController scrollController;

  @override
  void initState() {
    super.initState();
    scrollController = widget.scrollController ?? ScrollController();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (state != null) return;
    final walletCtx = WalletContext.of(context)!;
    final built = walletCtx.superWallet.buildTx(
      coord: coord,
      masterAppkey: walletCtx.masterAppkey,
    )!;
    built.setAccessId(
      accessId: built.accessStructures().first.accessStructureId(),
    );
    if (built.confirmationEstimates() == null) {
      built.refreshConfirmationEstimates();
    }
    final sub = built.subscribe();
    setState(() {
      state = built;
      this.sub = sub;
    });
    stateSub = sub.start().listen((_) => mounted ? setState(() {}) : null);
    WidgetsBinding.instance.addPostFrameCallback((_) => _plan());
  }

  @override
  void dispose() {
    stateSub?.cancel();
    sub?.dispose();
    state?.dispose();
    if (widget.scrollController == null) scrollController.dispose();
    super.dispose();
  }

  Future<void> _plan({bool repick = false}) async {
    final walletCtx = WalletContext.of(context)!;
    final state = this.state!;
    if (repick || state.feerate() == null) {
      await showDialog<ConfirmationTarget>(
        context: context,
        builder: (context) => BackdropFilter(
          filter: blurFilter,
          child: FeeRatePickerDialog(walletContext: walletCtx, state: state),
        ),
      );
      if (state.feerate() == null) {
        if (mounted) Navigator.pop(context);
        return;
      }
    }
    if (!mounted) return;
    try {
      final plan = widget.planner(state.feerate()!);
      setState(() {
        this.plan = plan;
        planError = null;
      });
    } catch (e) {
      // Drop the dead plan with it. The summary renders above the error, so its fee and change
      // would sit beside the feerate chip showing the rate that just failed.
      setState(() {
        plan = null;
        planError = e.toString();
      });
    }
  }

  Future<void> _sign(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;
    final fsCtx = FrostsnapContext.of(context)!;
    final UnsignedTx unsignedTx;
    try {
      unsignedTx = walletCtx.superWallet.commitSend(coord: coord, plan: plan!);
    } catch (e) {
      // A planned input was spent while this page was open: the plan is dead. Replan — the
      // input set is recomputed, not chosen.
      showErrorSnackbar(context, 'Wallet changed while building: $e');
      await _plan();
      return;
    }

    final access = walletCtx.wallet.frostKey()!.accessStructures()[0];
    final tx = unsignedTx.details();
    final txDetails = TxDetailsModel(
      tx: tx,
      chainTipHeight: walletCtx.wallet.superWallet.height(),
      now: DateTime.now(),
    );
    if (!context.mounted) return;
    Navigator.pop(context);
    await showBottomSheetOrDialog(
      context,
      title: Text('Transaction Details'),
      builder: (context, scrollController) => walletCtx.wrap(
        TxDetailsPage(
          scrollController: scrollController,
          txStates: walletCtx.txStream,
          txDetails: txDetails,
          psbtMan: fsCtx.psbtManager,
          signingParams: StartSigning(
            accessStructureRef: access.accessStructureRef(),
            unsignedTx: unsignedTx,
            devices: state!.selectedSigners().toList(),
          ),
        ),
      ),
    );
  }

  Future<void> refreshConfirmationEstimates() async {
    if (!mounted || estimateRunning) return;
    setState(() => estimateRunning = true);
    await state!.refreshConfirmationEstimates();
    if (mounted) setState(() => estimateRunning = false);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final state = this.state;
    final plan = this.plan;
    final cardColor = theme.colorScheme.surfaceContainerHigh;

    if (state == null || (plan == null && planError == null)) {
      return SizedBox(
        height: 180,
        child: Center(child: CircularProgressIndicator()),
      );
    }

    final confirmationBlocks = state.confirmationBlocksOfFeerate();
    final feerate = state.feerate();

    // The same control the send flow changes its feerate with, in the same place: on a
    // consolidation it is the only input, so it stays live rather than locking at the signer
    // step, and each change replans for free.
    final etaInputCard = TextButton.icon(
      onPressed: () => _plan(repick: true),
      icon: Stack(
        alignment: AlignmentDirectional.bottomCenter,
        children: [
          Icon(Icons.speed_rounded),
          if (estimateRunning)
            SizedBox(
              height: 2.0,
              width: 12.0,
              child: LinearProgressIndicator(),
            ),
        ],
      ),
      label: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Flexible(
            child: Text.rich(
              confirmationBlocks != null
                  ? TextSpan(
                      children: [
                        TextSpan(text: 'Confirms in '),
                        TextSpan(
                          text: '~${confirmationBlocks * 10} min',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                      ],
                    )
                  : TextSpan(text: 'Feerate'),
            ),
          ),
          if (feerate != null)
            Flexible(child: Text('${feerate.toStringAsFixed(1)} sat/vB')),
        ],
      ),
    );

    final Widget body;
    if (planError != null) {
      // Most plan failures are rate-dependent — the coins are admitted at a fixed bar, but the
      // picked rate can be too high for them to pay their own fee — so the one degree of freedom
      // stays available here.
      body = Card.outlined(
        color: cardColor,
        shape: cardShape(context),
        margin: EdgeInsets.all(0.0),
        child: Padding(
          padding: EdgeInsets.all(16.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'Couldn\'t build the consolidation: $planError',
                style: theme.textTheme.bodyMedium,
              ),
              SizedBox(height: 16),
              FilledButton(
                onPressed: () => _plan(repick: true),
                child: Text('Try a different feerate'),
              ),
            ],
          ),
        ),
      );
    } else {
      final threshold = state.accessStruct()!.threshold();
      final selected = state.selectedSigners();
      final remaining = threshold - selected.length;
      body = Card.outlined(
        color: cardColor,
        shape: cardShape(context),
        margin: EdgeInsets.all(0.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            ListTile(
              dense: true,
              title: Text('Select Signers'),
              trailing: Text('$threshold required'),
            ),
            Column(
              children: state.availableSigners().map((device) {
                final (id, name) = device;
                final nonces = coord.noncesAvailable(id: id);
                final isSelected = state.isSignerSelected(dId: id);

                if (nonces == 0) state.deselectSigner(dId: id);

                return CheckboxListTile(
                  value: isSelected,
                  onChanged: remaining > 0 || isSelected
                      ? (checked) => checked ?? false
                            ? state.selectSigner(dId: id)
                            : state.deselectSigner(dId: id)
                      : null,
                  secondary: Icon(Icons.key),
                  title: Text(name ?? '<unknown>'),
                  subtitle: nonces == 0
                      ? Text(
                          'no nonces remaining or too many signing sessions',
                          style: TextStyle(color: theme.colorScheme.error),
                        )
                      : null,
                );
              }).toList(),
            ),
            Padding(
              padding: const EdgeInsets.all(12.0),
              child: FilledButton(
                onPressed: remaining == 0 ? () => _sign(context) : null,
                child: Text(
                  remaining > 0 ? 'Select $remaining more' : 'Sign transaction',
                ),
              ),
            ),
          ],
        ),
      );
    }

    final description = widget.description;
    final coins = plan?.inputCount() ?? 0;
    final summary = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (description != null)
          Padding(
            padding: EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: Text(
              description,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        if (plan != null) ...[
          ListTile(
            onTap: () => _plan(repick: true),
            leading: summaryRowLabel(
              context,
              'Consolidating $coins coin${coins == 1 ? '' : 's'}',
            ),
            title: SatoshiText(value: plan.inputTotal()),
          ),
          ListTile(
            onTap: () => _plan(repick: true),
            leading: Row(
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.center,
              spacing: 4.0,
              children: [
                summaryRowLabel(context, 'Fee'),
                Flexible(
                  child: summaryRowChip(
                    context,
                    '${feerate?.toStringAsFixed(1) ?? '~'} sat/vB',
                  ),
                ),
              ],
            ),
            title: SatoshiText(
              value: plan.fee(),
              style: TextStyle(color: theme.colorScheme.secondary),
            ),
          ),
          ListTile(
            onTap: () => _plan(repick: true),
            leading: summaryRowLabel(context, 'Returning to this wallet'),
            title: SatoshiText(value: plan.changeValue()),
          ),
          SizedBox(height: 24.0),
        ],
      ],
    );

    return CustomScrollView(
      controller: scrollController,
      reverse: true,
      shrinkWrap: true,
      slivers: [
        SliverSafeArea(
          sliver: SliverToBoxAdapter(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                summary,
                Padding(
                  padding: EdgeInsets.fromLTRB(16.0, 0.0, 16.0, 8.0),
                  child: Column(
                    children: [
                      Padding(
                        padding: EdgeInsets.symmetric(vertical: 12.0),
                        child: etaInputCard,
                      ),
                      body,
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
