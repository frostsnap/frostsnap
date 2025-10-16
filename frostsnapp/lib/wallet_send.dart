import 'package:frostsnap/camera.dart';
import 'package:frostsnap/contexts.dart';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_send_controllers.dart';
import 'package:frostsnap/wallet_send_feerate_picker.dart';
import 'package:frostsnap/wallet_tx_details.dart';

enum SendPageIndex { recipient, amount, signers }

class DeleteSigningSession {}

class WalletSendPage extends StatefulWidget {
  final ScrollController? scrollController;
  const WalletSendPage({super.key, this.scrollController});

  @override
  State<WalletSendPage> createState() => _WalletSendPageState();
}

class _WalletSendPageState extends State<WalletSendPage> {
  static const sectionPadding = EdgeInsets.fromLTRB(16.0, 0.0, 16.0, 8.0);

  late final AddressInputController addressModel;
  late final FeeRateController feeRateModel;
  late final AmountAvaliableController amountAvaliable;
  late final AmountInputController amountModel;
  UnsignedTx? unsignedTx;
  final selectedDevicesModel = SelectedDevicesController();
  final signingSession = SigningSessionController();
  var pageIndex = SendPageIndex.recipient;
  late final ScrollController _scrollController;
  late final ValueNotifier<bool> _isAtEnd;
  late final Widget _recipientDoneButton;

  _initRecipientDoneButton() {
    _recipientDoneButton = ListenableBuilder(
      listenable: addressModel,
      builder: (context, _) => IconButton.filled(
        onPressed: addressModel.errorText != null
            ? null
            : () => recipientDone(context),
        icon: Icon(Icons.done),
      ),
    );
  }

  late final Widget _amountDoneButton;
  _initAmountDoneButton() {
    _amountDoneButton = ListenableBuilder(
      listenable: amountModel,
      builder: (context, _) => IconButton.filled(
        // TODO: Create a getter for this.
        onPressed:
            (amountModel.error != null ||
                amountModel.amount == null ||
                amountModel.textEditingController.text.isEmpty)
            ? null
            : () => amountDone(context),
        icon: Icon(Icons.done),
      ),
    );
  }

  late final Widget _signersDoneButton;
  _initSignersDoneButton() {
    _signersDoneButton = ListenableBuilder(
      listenable: selectedDevicesModel,
      builder: (context, child) {
        final isThresholdMet = selectedDevicesModel.isThresholdMet;
        final remaining = selectedDevicesModel.remaining;
        final nextText = (isThresholdMet)
            ? 'Sign Transaction'
            : 'Select $remaining more';
        return FilledButton(
          onPressed: (unsignedTx == null || !isThresholdMet)
              ? null
              : () => signersDone(context),
          child: Text(nextText),
        );
      },
    );
  }

  @override
  void initState() {
    super.initState();
    _isAtEnd = ValueNotifier(true);
    _scrollController = widget.scrollController ?? ScrollController();
    _scrollController.addListener(() {
      _isAtEnd.value =
          _scrollController.position.atEdge &&
          _scrollController.position.pixels ==
              _scrollController.position.maxScrollExtent;
    });

    addressModel = AddressInputController();
    feeRateModel = FeeRateController(satsPerVB: 5.0);

    amountAvaliable = AmountAvaliableController(
      feeRateController: feeRateModel,
    );
    amountModel = AmountInputController(
      amountAvailableController: amountAvaliable,
    );

    _initRecipientDoneButton();
    _initAmountDoneButton();
    _initSignersDoneButton();
  }

  @override
  void dispose() {
    amountModel.dispose();
    amountAvaliable.dispose();
    feeRateModel.dispose();
    addressModel.dispose();
    selectedDevicesModel.dispose();
    signingSession.dispose();
    if (widget.scrollController == null) _scrollController.dispose();
    _isAtEnd.dispose();
    super.dispose();
  }

  bool alreadyRefreshed = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (context.mounted) {
      final walletCtx = WalletContext.of(context);
      if (walletCtx != null) {
        selectedDevicesModel.walletContext = walletCtx;
        amountAvaliable.walletContext = walletCtx;
        if (!alreadyRefreshed) {
          feeRateModel.refreshEstimates(context, walletCtx, 1);
          alreadyRefreshed = true;
        }
      }
    }
  }

  Widget _buildCompletedList(BuildContext context) {
    final theme = Theme.of(context);

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
              title: ListenableBuilder(
                listenable: addressModel,
                builder: (ctx, _) => Text(
                  addressModel.formattedAddress,
                  textWidthBasis: TextWidthBasis.longestLine,
                  textAlign: TextAlign.right,
                  style: monospaceTextStyle,
                ),
              ),
            ),
          if (pageIndex.index > SendPageIndex.amount.index)
            Column(
              children: [
                ListTile(
                  onTap: () => setState(() => pageIndex = SendPageIndex.amount),
                  leading: completedCardLabel(context, 'Amount'),
                  title: SatoshiText(value: amountModel.amount ?? 0),
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
                        child: Card(
                          color: theme.colorScheme.secondaryContainer,
                          child: Padding(
                            padding: EdgeInsets.symmetric(
                              vertical: 2.0,
                              horizontal: 6.0,
                            ),
                            child: Text(
                              '${unsignedTx?.feerate()?.toStringAsFixed(1)} sat/vB',
                              style: theme.textTheme.labelSmall?.copyWith(
                                color: theme.colorScheme.onSecondaryContainer,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                  title: SatoshiText(
                    value: unsignedTx?.fee(),
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                ),
              ],
            ),
          if (pageIndex.index > SendPageIndex.recipient.index)
            SizedBox(height: 24.0),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final etaInputCard = ListenableBuilder(
      listenable: feeRateModel,
      builder: (context, _) {
        return TextButton.icon(
          onPressed: pageIndex.index < SendPageIndex.signers.index
              ? () => showFeeRateDialog(context)
              : null,
          icon: Stack(
            alignment: AlignmentDirectional.bottomCenter,
            children: [
              Icon(Icons.speed_rounded),
              if (feeRateModel.estimateRunning)
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
                  TextSpan(
                    children: [
                      TextSpan(text: 'Confirms in '),
                      TextSpan(
                        text: feeRateModel.targetTime == null
                            ? '...'
                            : '~${feeRateModel.targetTime} min',
                        style: TextStyle(fontWeight: FontWeight.bold),
                      ),
                    ],
                  ),
                ),
              ),
              if (pageIndex.index < SendPageIndex.signers.index)
                Flexible(child: Text('${feeRateModel.satsPerVB} sat/vB')),
            ],
          ),
        );
      },
    );

    final cardColor = theme.colorScheme.surfaceContainerHigh;

    final recipientInputCard = Card.outlined(
      color: cardColor,
      margin: EdgeInsets.all(0.0),
      child: Padding(
        padding: EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12.0,
          children: [
            AddressInput(
              controller: addressModel,
              onSubmitted: (_) => recipientDone(context),
              decoration: InputDecoration(
                filled: false,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8.0),
                  borderSide: BorderSide.none,
                ),
                hintText: 'Recipient',
                errorMaxLines: 2,
              ),
            ),
            Row(
              spacing: 8.0,
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  spacing: 8.0,
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
                _recipientDoneButton,
              ],
            ),
          ],
        ),
      ),
    );

    final amountInputCard = Card.outlined(
      color: cardColor,
      margin: EdgeInsets.all(0.0),
      child: Padding(
        padding: EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12.0,
          children: [
            AmountInput(
              model: amountModel,
              onSubmitted: (_) => amountDone(context),
              decoration: InputDecoration(
                filled: false,
                errorMaxLines: 2,
                hintText: 'Amount',
              ),
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                ListenableBuilder(
                  listenable: Listenable.merge([amountModel, amountAvaliable]),
                  builder: (context, _) => TextButton.icon(
                    onPressed:
                        (amountAvaliable.value == null ||
                            amountAvaliable.value! == 0)
                        ? null
                        : () => amountModel.sendMax = !amountModel.sendMax,
                    label: Row(
                      spacing: 4.0,
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Text('Send Max'),
                        SatoshiText(value: amountAvaliable.value),
                      ],
                    ),
                    icon: Icon(
                      amountModel.sendMax
                          ? Icons.check_box
                          : Icons.check_box_outline_blank,
                    ),
                  ),
                ),
                _amountDoneButton,
              ],
            ),
          ],
        ),
      ),
    );

    final signersInputCard = Card.outlined(
      color: cardColor,
      margin: EdgeInsets.all(0.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          ListTile(
            dense: true,
            title: Text('Select Signers'),
            trailing: Text('${selectedDevicesModel.threshold} required'),
          ),
          ListenableBuilder(
            listenable: selectedDevicesModel,
            builder: (context, child) => Column(
              children: selectedDevicesModel.devices.map((device) {
                if (device.nonces == 0) {
                  selectedDevicesModel.deselect(device.id);
                }
                return CheckboxListTile(
                  value: device.selected,
                  onChanged: device.canSelect
                      ? (selected) => selected ?? false
                            ? selectedDevicesModel.select(device.id)
                            : selectedDevicesModel.deselect(device.id)
                      : null,
                  secondary: Icon(Icons.key),
                  title: Text(device.name ?? '<unknown>'),
                  subtitle: device.nonces == 0
                      ? Text(
                          'no nonces remaining or too many signing sessions',
                          style: TextStyle(color: theme.colorScheme.error),
                        )
                      : null,
                );
              }).toList(),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(12.0),
            child: _signersDoneButton,
          ),
        ],
      ),
    );

    final mediaQuery = MediaQuery.of(context);
    final scrollView = CustomScrollView(
      controller: _scrollController,
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
                      if (pageIndex == SendPageIndex.recipient)
                        recipientInputCard,
                      if (pageIndex == SendPageIndex.amount) amountInputCard,
                      if (pageIndex == SendPageIndex.signers) signersInputCard,
                      //if (pageIndex == SendPageIndex.sign) signInputCard,
                      Padding(
                        padding: EdgeInsets.symmetric(vertical: 12.0),
                        child: etaInputCard,
                      ),
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

  showFeeRateDialog(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final fut = showDialog<double>(
      context: context,
      builder: (context) {
        return BackdropFilter(
          filter: blurFilter,
          child: FeeRatePickerDialog(
            walletContext: walletCtx,
            addressModel: addressModel,
            amountModel: amountModel,
            feeRateModel: feeRateModel,
          ),
        );
      },
    );
    fut.then((_) {
      if (context.mounted && pageIndex.index > SendPageIndex.amount.index) {
        // TODO: Ideally we want to be able to update the review page.
        setState(() => pageIndex = SendPageIndex.amount);
      }
    });
  }

  signersDone(BuildContext context) async {
    if (unsignedTx == null) return;

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
          devices: selectedDevicesModel.selected.toList(),
          psbtMan: fsCtx.psbtManager,
        ),
      ),
    );
  }

  amountDone(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final address = addressModel.address!;
    final amount = amountModel.amount!;
    final feerate = feeRateModel.satsPerVB;

    final unsignedTxFut = walletCtx.wallet.superWallet.sendTo(
      masterAppkey: walletCtx.masterAppkey,
      toAddress: address,
      value: amount,
      feerate: feerate,
    );
    unsignedTxFut.then((unsignedTx) {
      this.unsignedTx = unsignedTx;
      nextPageOrPop(null);
    }, onError: (e) => amountModel.customError = e.toString());
  }

  recipientDone(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;
    if (await addressModel.submit(walletCtx)) {
      // Pre-populate amount if existed in URI (user can still edit)
      if (addressModel.amount != null) {
        amountModel.textEditingController.text = addressModel.amount.toString();
      }

      nextPageOrPop(null);
    }
  }

  recipientPaste(BuildContext context) async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (!context.mounted || data == null || data.text == null) return;
    addressModel.controller.text = data.text!;
    recipientDone(context);
  }

  recipientScan(BuildContext context) async {
    final addressResult = await showDialog<String>(
      context: context,
      builder: (context) => AddressScanner(),
    );
    if (!context.mounted || addressResult == null) return;
    addressModel.controller.text = addressResult;
    recipientDone(context);
  }

  scrollToTop() {
    Future.delayed(Durations.long3).then((_) async {
      if (context.mounted) {
        await _scrollController.animateTo(
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
