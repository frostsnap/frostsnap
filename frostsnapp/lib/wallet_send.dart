import 'package:frostsnap/camera/camera.dart';
import 'package:frostsnap/contexts.dart';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/broadcast.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_send_controllers.dart';
import 'package:frostsnap/wallet_send_feerate_picker.dart';
import 'package:frostsnap/wallet_tx_details.dart';

enum SendPageIndex { recipient, amount, signers }

class WalletSendPage extends StatefulWidget {
  final SuperWallet superWallet;
  final MasterAppkey masterAppkey;
  final double initialFeerate;
  final ScrollController? scrollController;
  const WalletSendPage({
    super.key,
    required this.superWallet,
    required this.masterAppkey,
    this.initialFeerate = 3.0,
    this.scrollController,
  });

  BuildTxState buildTx() {
    final state = superWallet.buildTx(coord: coord, masterAppkey: masterAppkey);
    // This can only fail if the wallet does not have a frost key - in which case, we shouldn't even
    // be sending from this wallet!
    return state!;
  }

  @override
  State<WalletSendPage> createState() => _WalletSendPageState();
}

class _WalletSendPageState extends State<WalletSendPage> {
  static const sectionPadding = EdgeInsets.fromLTRB(16.0, 0.0, 16.0, 8.0);

  late final UnitBroadcastSubscription sub;

  late final ScrollController scrollController;
  late final BuildTxState state;

  late final AddressInputController addrController;
  String? addrError;

  late final AmountInputController amountController;

  var pageIndex = SendPageIndex.recipient;
  bool estimateRunning = false;

  @override
  void initState() {
    super.initState();

    scrollController = widget.scrollController ?? ScrollController();

    state = widget.buildTx();

    // We only support one access structure for now.
    state.setAccessId(
      accessId: state.accessStructures().first.accessStructureId(),
    );
    if (state.confirmationEstimates() == null)
      state.refreshConfirmationEstimates();

    sub = state.subscribe();
    sub.start().listen((_) => mounted ? setState(() {}) : null);

    addrController = AddressInputController(state);
    amountController = AmountInputController(state: state);
  }

  @override
  void dispose() {
    amountController.dispose();
    addrController.dispose();
    sub.dispose();
    state.dispose();
    if (widget.scrollController == null) scrollController.dispose();
    super.dispose();
  }

  bool alreadyRefreshed = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!mounted) return;
    if (state.confirmationEstimates() == null) {
      try {
        state.refreshConfirmationEstimates();
      } catch (e, _) {
        // Ignore in production, log in debug.
        assert(() {
          print('Ignored exception: $e');
          return true;
        }());
      }
    }
  }

  Widget _buildCompletedAmountAndFee(BuildContext context) {
    final theme = Theme.of(context);

    final isSendMax = state.isSendMax(recipient: 0);

    int? amount;
    try {
      amount = state.amount(recipient: 0);
    } on AmountError catch (e) {
      assert(() {
        print('Must have valid amount at this point: $e');
        return true;
      }());
      prevPageOrPop(null);
    }

    Widget leadingCard(String data) {
      return Card(
        color: theme.colorScheme.secondaryContainer,
        child: Padding(
          padding: EdgeInsets.symmetric(vertical: 2.0, horizontal: 6.0),
          child: Text(
            data,
            style: theme.textTheme.labelSmall?.copyWith(
              color: theme.colorScheme.onSecondaryContainer,
            ),
          ),
        ),
      );
    }

    return Column(
      children: [
        ListTile(
          onTap: () => setState(() => pageIndex = SendPageIndex.amount),
          leading: Row(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.center,
            spacing: 4.0,
            children: [
              completedCardLabel(context, 'Amount'),
              if (isSendMax) Flexible(child: leadingCard('Max')),
            ],
          ),
          title: SatoshiText(value: amount),
        ),
        ListTile(
          onTap: () => showFeeRateDialog(context),
          leading: Row(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.center,
            spacing: 4.0,
            children: [
              completedCardLabel(context, 'Fee'),
              Flexible(
                child: leadingCard(
                  '${state.feerate()?.toStringAsFixed(1) ?? '~'} sat/vB',
                ),
              ),
            ],
          ),
          title: SatoshiText(
            value: state.fee(),
            style: TextStyle(color: theme.colorScheme.secondary),
          ),
        ),
      ],
    );
  }

  Widget _buildCompletedList(BuildContext context) {
    // TODO: We only support one recipient right now!
    final recipient = state.recipient(recipient: 0);

    return AnimatedSize(
      duration: Durations.short4,
      curve: Curves.easeInOutCubicEmphasized,
      alignment: Alignment.topCenter,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (pageIndex.index > SendPageIndex.recipient.index)
            ListTile(
              onTap: () => setState(() => pageIndex = SendPageIndex.recipient),
              leading: completedCardLabel(context, 'Recipient'),
              title: Text(
                // addressModel.formattedAddress,
                spacedHex(recipient?.address?.toString() ?? '', groupSize: 4),
                textWidthBasis: TextWidthBasis.longestLine,
                textAlign: TextAlign.right,
                style: monospaceTextStyle,
              ),
            ),
          if (pageIndex.index > SendPageIndex.amount.index)
            _buildCompletedAmountAndFee(context),
          if (pageIndex.index > SendPageIndex.recipient.index)
            SizedBox(height: 24.0),
        ],
      ),
    );
  }

  void refreshConfirmationEstimates() async {
    if (!mounted) return;
    if (estimateRunning) return;
    setState(() => estimateRunning = true);
    await state.refreshConfirmationEstimates();
    if (mounted) setState(() => estimateRunning = false);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final confirmationBlocks = state.confirmationBlocksOfFeerate();
    final feerate = state.feerate();

    final etaInputCard = TextButton.icon(
      onPressed: pageIndex.index < SendPageIndex.signers.index
          ? () => showFeeRateDialog(context)
          : null,
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
          if (pageIndex.index < SendPageIndex.signers.index && feerate != null)
            Flexible(child: Text('${feerate.toStringAsFixed(1)} sat/vB')),
        ],
      ),
    );

    final cardColor = theme.colorScheme.surfaceContainerHigh;

    final recipientInputCard = Card.outlined(
      color: cardColor,
      shape: cardShape(context),
      margin: EdgeInsets.all(0.0),
      child: Padding(
        padding: EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12.0,
          children: [
            AddressInput(
              controller: addrController,
              onSubmitted: (_) => recipientDone(context),
              decoration: InputDecoration(
                filled: false,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8.0),
                  borderSide: BorderSide.none,
                ),
                hintText: 'Recipient',
                errorText: addrError,
                errorMaxLines: 2,
              ),
            ),
            Row(
              spacing: 8.0,
              children: [
                Expanded(
                  child: SizedBox(
                    // Constrain height - horizontal ListView requires fixed vertical height.
                    height: 36,
                    child: ListView(
                      shrinkWrap: true,
                      scrollDirection: Axis.horizontal,
                      children: [
                        TextButton.icon(
                          onPressed: () => recipientPaste(context),
                          label: Text('Paste'),
                          icon: Icon(Icons.paste),
                        ),
                        TextButton.icon(
                          onPressed: () => recipientScan(context),
                          label: Text('Scan'),
                          icon: Icon(Icons.qr_code),
                        ),
                      ],
                    ),
                  ),
                ),
                IconButton.filled(
                  onPressed: addrController.errorText != null
                      ? null
                      : () => recipientDone(context),
                  icon: Icon(Icons.done),
                ),
              ],
            ),
          ],
        ),
      ),
    );

    final availableAmount = state.availableAmount(recipient: 0);

    int? amount;
    String? amountErr;
    try {
      amount = state.amount(recipient: 0);
    } on AmountError catch (e) {
      amountErr = switch (e) {
        AmountError_UnspecifiedFeerate() => 'No feerate set.',
        AmountError_UnspecifiedAmount() => 'No amount set.',
        AmountError_NoAmountAvailable() => 'No balance available.',
        AmountError_TargetExceedsAvailable(:final target, :final available) =>
          'Exceeds max by ${target - available}sat.',
        AmountError_UnspecifiedAddress() => 'No recipient address.',
        AmountError_AmountBelowDust(:final minNonDust) =>
          'Minimum amount is $minNonDust.',
      };
    }

    final amountInputCard = Card.outlined(
      color: cardColor,
      shape: cardShape(context),
      margin: EdgeInsets.all(0.0),
      child: Padding(
        padding: EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12.0,
          children: [
            if (!state.isSendMax(recipient: 0))
              AmountInput(
                model: amountController,
                onSubmitted: (_) => amountDone(context),
                decoration: InputDecoration(
                  filled: false,
                  errorMaxLines: 2,
                  hintText: 'Amount',
                  errorText: amountErr,
                ),
              ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                TextButton.icon(
                  onPressed: (availableAmount ?? 0) == 0
                      ? null
                      : () => state.toggleSendMax(
                          recipient: 0,
                          fallbackAmount: amountController.amount,
                        ),
                  label: Row(
                    spacing: 4.0,
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text('Send Max'),
                      SatoshiText(value: availableAmount),
                    ],
                  ),
                  icon: Icon(
                    (state.recipient(recipient: 0)?.amount?.isSendMax() ??
                            false)
                        ? Icons.check_box
                        : Icons.check_box_outline_blank,
                  ),
                ),
                IconButton.filled(
                  onPressed: amount == null ? null : () => amountDone(context),
                  icon: Icon(Icons.done),
                ),
              ],
            ),
          ],
        ),
      ),
    );

    final accessStruct = state.accessStruct()!;
    final threshold = accessStruct.threshold();
    final selectedDevices = state.selectedSigners();
    final remaining = threshold - selectedDevices.length;

    final signersInputCard = Card.outlined(
      color: cardColor,
      shape: cardShape(context),
      margin: EdgeInsets.all(0.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          ListTile(
            dense: true,
            title: Text('Select Signers'),
            trailing: Text('${threshold} required'),
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
                    ? (selected) => selected ?? false
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
              onPressed: remaining == 0 ? () => signersDone(context) : null,
              child: Text(
                remaining > 0 ? 'Select ${remaining} more' : 'Sign transaction',
              ),
            ),
          ),
        ],
      ),
    );

    final mediaQuery = MediaQuery.of(context);
    final scrollView = CustomScrollView(
      controller: scrollController,
      reverse: true,
      shrinkWrap: true,
      slivers: [
        SliverSafeArea(
          sliver: SliverToBoxAdapter(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _buildCompletedList(context),
                Padding(
                  padding: sectionPadding.add(
                    EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom),
                  ),
                  child: Column(
                    children: [
                      Padding(
                        padding: EdgeInsets.symmetric(vertical: 12.0),
                        child: etaInputCard,
                      ),
                      if (pageIndex == SendPageIndex.recipient)
                        recipientInputCard,
                      if (pageIndex == SendPageIndex.amount) amountInputCard,
                      if (pageIndex == SendPageIndex.signers) signersInputCard,
                      //if (pageIndex == SendPageIndex.sign) signInputCard,
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );

    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        prevPageOrPop(result);
      },
      child: scrollView,
    );
  }

  Widget completedCardLabel(BuildContext context, String text) =>
      Text(text, style: Theme.of(context).textTheme.labelLarge);

  Future<ConfirmationTarget?> showFeeRateDialog(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;

    final target = await showDialog<ConfirmationTarget>(
      context: context,
      builder: (context) {
        return BackdropFilter(
          filter: blurFilter,
          child: FeeRatePickerDialog(walletContext: walletCtx, state: state),
        );
      },
    );

    if (context.mounted && pageIndex.index > SendPageIndex.amount.index) {
      setState(() => pageIndex = SendPageIndex.amount);
    }
    return target;
  }

  signersDone(BuildContext context) async {
    UnsignedTx? unsignedTx;
    try {
      unsignedTx = state.tryFinish();
    } on TryFinishTxError catch (e) {
      final why = switch (e) {
        TryFinishTxError.missingFeerate => 'No feerate',
        TryFinishTxError.incompleteRecipientValues => 'No recipient amount',
        TryFinishTxError.insufficientBalance => 'Insufficient Balance',
      };
      showErrorSnackbar(context, 'Invalid transaction: $why');
    }

    final fsCtx = FrostsnapContext.of(context)!;
    final walletCtx = WalletContext.of(context)!;
    final access = walletCtx.wallet.frostKey()!.accessStructures()[0];
    final chainTipHeight = walletCtx.wallet.superWallet.height();
    final now = DateTime.now();

    final tx = unsignedTx?.details(
      superWallet: walletCtx.superWallet,
      masterAppkey: walletCtx.masterAppkey,
    );
    if (tx == null) return;
    final txDetails = TxDetailsModel(
      tx: tx,
      chainTipHeight: chainTipHeight,
      now: now,
    );
    nextPageOrPop(null);
    await showBottomSheetOrDialog(
      context,
      title: Text('Transaction Details'),
      builder: (context, scrollController) => walletCtx.wrap(
        TxDetailsPage.startSigning(
          txStates: walletCtx.txStream,
          txDetails: txDetails,
          accessStructureRef: access.accessStructureRef(),
          unsignedTx: unsignedTx!,
          devices: state.selectedSigners().toList(),
          psbtMan: fsCtx.psbtManager,
        ),
      ),
    );
  }

  void amountDone(BuildContext context) {
    nextPageOrPop(null);
  }

  void recipientDone(BuildContext context) async {
    // Hardcoded recipient index for now - since only 1 recipient is supported.
    final submitOkay = addrController.submit(0);
    if (!submitOkay) {
      // So that we see the address input error.
      setState(() {});
      return;
    }
    if (state.feerate() == null) {
      final feerate = await showFeeRateDialog(context);
      if (feerate == null) return;
    }

    nextPageOrPop(null);
  }

  recipientPaste(BuildContext context) async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (!context.mounted || data == null || data.text == null) return;
    addrController.controller.text = data.text!;
    recipientDone(context);
  }

  recipientScan(BuildContext context) async {
    final addressResult = await MaybeFullscreenDialog.show<String>(
      context: context,
      child: AddressScanner(),
    );
    if (!context.mounted || addressResult == null) return;
    addrController.controller.text = addressResult;
    recipientDone(context);
  }

  scrollToTop() {
    Future.delayed(Durations.long3).then((_) async {
      if (context.mounted) {
        await scrollController.animateTo(
          0,
          duration: Durations.short3,
          curve: Curves.linear,
        );
      }
    });
  }

  prevPageOrPop(Object? result) {
    final prevIndex = pageIndex.index - 1;
    if (prevIndex < 0) {
      Navigator.pop(context, result);
    } else {
      final prev = SendPageIndex.values[prevIndex];
      setState(() => pageIndex = prev);
      if (pageIndex == SendPageIndex.signers) scrollToTop();
    }
  }

  nextPageOrPop(Object? result) {
    final nextIndex = pageIndex.index + 1;
    if (nextIndex >= SendPageIndex.values.length) {
      Navigator.pop(context, result);
    } else {
      final next = SendPageIndex.values[nextIndex];
      setState(() => pageIndex = next);
      if (pageIndex == SendPageIndex.signers) scrollToTop();
    }
  }
}
