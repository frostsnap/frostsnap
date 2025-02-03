import 'dart:io';
import 'package:frostsnapp/contexts.dart';
import 'package:camera/camera.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/cached_future.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/wallet_send_controllers.dart';
import 'package:frostsnapp/wallet_send_feerate_picker.dart';
import 'package:frostsnapp/wallet_send_scan.dart';

enum SendPageIndex {
  recipient,
  amount,
  sign,
}

class WalletSendPage extends StatefulWidget {
  final ScrollController? scrollController;
  const WalletSendPage({super.key, this.scrollController});

  @override
  State<WalletSendPage> createState() => _WalletSendPageState();
}

class _WalletSendPageState extends State<WalletSendPage> {
  static const sectionPadding = EdgeInsets.fromLTRB(16.0, 0.0, 16.0, 8.0);

  final inputCardTopBorder = RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(12.0),
  );
  final inputCardBottomBorder = RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(12.0),
  );

  late final CachedFuture<List<CameraDescription>> cameras;

  late final AddressInputController addressModel;
  late final FeeRateController feeRateModel;
  late final AmountAvaliableController amountAvaliable;
  late final AmountInputController amountModel;

  UnsignedTx? unsignedTx;
  final selectedDevicesModel = SelectedDevicesController();

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
        onPressed: (amountModel.error != null ||
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
        final nextText =
            (isThresholdMet) ? 'Sign Transaction' : 'Select $remaining more';
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
      _isAtEnd.value = _scrollController.position.atEdge &&
          _scrollController.position.pixels ==
              _scrollController.position.maxScrollExtent;
    });

    cameras = CachedFuture(
      availableCameras().catchError((e) => <CameraDescription>[]),
    );

    addressModel = AddressInputController();
    feeRateModel = FeeRateController(satsPerVB: 5.0);

    amountAvaliable =
        AmountAvaliableController(feeRateController: feeRateModel);
    amountModel =
        AmountInputController(amountAvailableController: amountAvaliable);

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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final appBar = SliverPadding(
      padding: EdgeInsets.symmetric(vertical: 0.0, horizontal: 0.0),
      sliver: SliverAppBar(
        title: Text('Send Bitcoin'),
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
      ),
    );

    final completedInfoCard = AnimatedSize(
      duration: Durations.short4,
      curve: Curves.easeInOutCubicEmphasized,
      alignment: Alignment.topCenter,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (pageIndex.index > SendPageIndex.recipient.index)
            ListTile(
              onTap: () => setState(() => pageIndex = SendPageIndex.recipient),
              leading: completedCardLabel(context, 'To Recipient'),
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
                    style: TextStyle(color: theme.colorScheme.onErrorContainer),
                  ),
                ),
              ],
            ),
          if (pageIndex.index > SendPageIndex.recipient.index)
            SizedBox(height: 24.0),
        ],
      ),
    );

    final Color mainCardColor = theme.colorScheme.surfaceContainerHigh;

    final etaInputCard = ListenableBuilder(
      listenable: feeRateModel,
      builder: (context, _) {
        return TextButton.icon(
          onPressed: pageIndex.index < SendPageIndex.sign.index
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
                )
            ],
          ),
          label: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                child: Text.rich(
                  TextSpan(children: [
                    TextSpan(text: 'Confirms in '),
                    TextSpan(
                      text: feeRateModel.targetTime == null
                          ? '...'
                          : '~${feeRateModel.targetTime} min',
                      style: TextStyle(fontWeight: FontWeight.bold),
                    ),
                  ]),
                ),
              ),
              if (pageIndex.index < SendPageIndex.sign.index)
                Flexible(child: Text('${feeRateModel.satsPerVB} sat/vB')),
            ],
          ),
        );
      },
    );

    final recipientInputCard = Card.filled(
      color: mainCardColor,
      shape: inputCardTopBorder,
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
                filled: true,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8.0),
                  borderSide: BorderSide.none,
                ),
                labelText: 'Recipient',
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
            )
          ],
        ),
      ),
    );

    final amountInputCard = Card.filled(
      color: mainCardColor,
      shape: inputCardTopBorder,
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
                filled: true,
                errorMaxLines: 2,
                labelText: 'Amount',
              ),
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                ListenableBuilder(
                  listenable: Listenable.merge([amountModel, amountAvaliable]),
                  builder: (context, _) => TextButton.icon(
                    onPressed: (amountAvaliable.value == null ||
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
                    icon: Icon(amountModel.sendMax
                        ? Icons.check_box
                        : Icons.check_box_outline_blank),
                  ),
                ),
                _amountDoneButton,
              ],
            ),
          ],
        ),
      ),
    );

    final signersInputCard = Card.filled(
      color: mainCardColor,
      margin: EdgeInsets.all(0.0),
      child: Padding(
        padding: const EdgeInsets.all(12.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12.0,
          children: [
            Padding(
              padding: EdgeInsets.all(0.0),
              child: Row(
                spacing: 8.0,
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    'Select Signers',
                    style: theme.textTheme.labelMedium
                        ?.copyWith(color: theme.colorScheme.primary),
                  ),
                  Text(
                    '${selectedDevicesModel.threshold} required',
                    style: theme.textTheme.labelMedium
                        ?.copyWith(color: theme.dividerColor),
                  ),
                ],
              ),
            ),
            ListenableBuilder(
              listenable: selectedDevicesModel,
              builder: (context, child) => Column(
                children: selectedDevicesModel.devices.map(
                  (device) {
                    if (device.nonces == 0) {
                      selectedDevicesModel.deselect(device.id);
                    }
                    return ListTile(
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(8.0),
                      ),
                      leading: Icon(device.selected
                          ? Icons.check_box
                          : Icons.check_box_outline_blank),
                      title: Text(device.name ?? '<unknown>'),
                      trailing: device.nonces == 0
                          ? Text(
                              'no nonces remaining',
                              style: TextStyle(color: theme.colorScheme.error),
                            )
                          : null,
                      enabled: device.canSelect,
                      selected: device.selected,
                      onTap: () => device.selected
                          ? selectedDevicesModel.deselect(device.id)
                          : selectedDevicesModel.select(device.id),
                    );
                  },
                ).toList(),
              ),
            ),
            Row(
              spacing: 8.0,
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                _signersDoneButton,
              ],
            ),
          ],
        ),
      ),
    );

    final scrollView = CustomScrollView(
      controller: _scrollController,
      reverse: true,
      shrinkWrap: true,
      slivers: [
        SliverToBoxAdapter(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              completedInfoCard,
              Padding(
                padding: sectionPadding,
                child: Column(
                  children: [
                    if (pageIndex == SendPageIndex.recipient)
                      recipientInputCard,
                    if (pageIndex == SendPageIndex.amount) amountInputCard,
                    if (pageIndex == SendPageIndex.sign) signersInputCard,
                    SizedBox(height: 12.0),
                    //if (pageIndex.index < SendPageIndex.sign.index)
                    etaInputCard,
                  ],
                ),
              ),
              SizedBox(height: MediaQuery.of(context).viewInsets.bottom),
            ],
          ),
        ),
        appBar,
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

  Widget completedCardLabel(BuildContext context, String text) => Text(
        text,
        style: Theme.of(context).textTheme.labelLarge,
      );

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

  signersDone(BuildContext context) {
    if (unsignedTx == null) return;
    selectedDevicesModel.signAndBroadcast(
      context,
      unsignedTx!,
      () => nextPageOrPop(null),
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
    unsignedTxFut.then(
      (unsignedTx) {
        this.unsignedTx = unsignedTx;
        nextPageOrPop(null);
      },
      onError: (e) => amountModel.customError =
          e.toString().replaceFirst('FrbAnyhowException(', ''),
    );
  }

  recipientDone(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    if (addressModel.submit(walletCtx)) {
      amountAvaliable.targetAddresses = [addressModel.controller.text];
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
      builder: (context) => FutureBuilder<List<CameraDescription>>(
        future: cameras.value,
        builder: (context, snapshot) => BackdropFilter(
          filter: blurFilter,
          child: Dialog(
            child:
                SendScanBody(cameras: snapshot.data ?? [], initialSelected: 0),
          ),
        ),
      ),
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
    final SendPageIndex? prevIndex;
    switch (pageIndex) {
      case SendPageIndex.recipient:
        prevIndex = null;
      case SendPageIndex.amount:
        prevIndex = SendPageIndex.recipient;
      case SendPageIndex.sign:
        prevIndex = SendPageIndex.amount;
      //case SendPageIndex.broadcast:
      //  prevIndex = SendPageIndex.sign;
    }
    if (prevIndex != null) {
      setState(() => pageIndex = prevIndex!);
      if (pageIndex == SendPageIndex.sign) scrollToTop();
    } else {
      Navigator.pop(context, result);
    }
  }

  nextPageOrPop(Object? result) {
    final SendPageIndex? nextIndex;
    switch (pageIndex) {
      case SendPageIndex.recipient:
        nextIndex = SendPageIndex.amount;
      case SendPageIndex.amount:
        nextIndex = SendPageIndex.sign;
      case SendPageIndex.sign:
        nextIndex = null;
    }
    if (nextIndex != null) {
      setState(() => pageIndex = nextIndex!);
      if (pageIndex == SendPageIndex.sign) scrollToTop();
    } else {
      Navigator.pop(context, result);
    }
  }
}

Future<void> signAndBroadcastWorkflowDialog(
    {required BuildContext context,
    required Stream<SigningState> signingStream,
    required UnsignedTx unsignedTx,
    required SuperWallet superWallet,
    required MasterAppkey masterAppkey,
    Function()? onBroadcastNewTx}) async {
  final effect = unsignedTx.effect(
      masterAppkey: masterAppkey, network: superWallet.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(context, effect),
  );
  if (signatures != null) {
    final signedTx = await unsignedTx.complete(signatures: signatures);
    if (context.mounted) {
      final wasBroadcast = await showBroadcastConfirmDialog(context,
          masterAppkey: masterAppkey, tx: signedTx, superWallet: superWallet);
      if (wasBroadcast) {
        onBroadcastNewTx?.call();
      }
    }
  }
}

class EffectTable extends StatelessWidget {
  final EffectOfTx effect;
  const EffectTable({super.key, required this.effect});

  @override
  Widget build(BuildContext context) {
    List<TableRow> transactionRows =
        effect.foreignReceivingAddresses.map((entry) {
      final (address, value) = entry;
      return TableRow(
        children: [
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: Text('Send to $address'),
          ),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: -value),
          ),
        ],
      );
    }).toList();

    transactionRows.add(
      TableRow(
        children: [
          Padding(
              padding: const EdgeInsets.all(8.0),
              child: effect.feerate != null
                  ? Text("${effect.feerate!.toStringAsFixed(1)} (sats/vb))")
                  : Text("unknown")),
          Padding(
              padding: const EdgeInsets.all(8.0),
              child: SatoshiText.withSign(value: -effect.fee)),
        ],
      ),
    );

    transactionRows.add(
      TableRow(
        children: [
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: Text('Net value'),
          ),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: effect.netValue),
          ),
        ],
      ),
    );

    final effectTable = Table(
      columnWidths: const {
        0: FlexColumnWidth(4),
        1: FlexColumnWidth(2),
      },
      border: TableBorder.all(),
      children: transactionRows,
    );

    final effectWidget = Column(
      children: [
        describeEffect(context, effect),
        Divider(),
        effectTable,
      ],
    );

    return effectWidget;
  }
}

Widget describeEffect(BuildContext context, EffectOfTx effect) {
  final style =
      DefaultTextStyle.of(context).style.copyWith(fontWeight: FontWeight.w600);
  final Widget description;

  if (effect.foreignReceivingAddresses.length == 1) {
    final (dest, amount) = effect.foreignReceivingAddresses[0];
    description = Wrap(
      direction: Axis.horizontal,
      children: <Widget>[
        Text('Sending '),
        SatoshiText(value: amount, style: style),
        Text(' to '),
        Text(
          dest,
          style: style,
        )
      ],
    );
  } else if (effect.foreignReceivingAddresses.isEmpty) {
    description = Text("Internal transfer");
  } else {
    description = Text("cannot describe this yet");
  }

  return description;
}

Future<bool> showBroadcastConfirmDialog(BuildContext context,
    {required MasterAppkey masterAppkey,
    required SignedTx tx,
    required SuperWallet superWallet}) async {
  final wasBroadcast = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        final effect =
            tx.effect(masterAppkey: masterAppkey, network: superWallet.network);
        final effectWidget = EffectTable(effect: effect);
        return AlertDialog(
            title: Text("Broadcast?"),
            content: SizedBox(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                child: Align(
                  alignment: Alignment.center,
                  child: effectWidget,
                )),
            actions: [
              ElevatedButton(
                  onPressed: () {
                    if (dialogContext.mounted) {
                      Navigator.pop(dialogContext, false);
                    }
                  },
                  child: Text("Cancel")),
              ElevatedButton(
                  onPressed: () async {
                    try {
                      await superWallet.broadcastTx(
                          masterAppkey: masterAppkey, tx: tx);
                      if (dialogContext.mounted) {
                        Navigator.pop(context, true);
                      }
                    } catch (e) {
                      if (dialogContext.mounted) {
                        Navigator.pop(dialogContext, false);
                        showErrorSnackbarTop(
                            dialogContext, "Broadcast error: $e");
                      }
                    }
                  },
                  child: Text("Broadcast"))
            ]);
      });

  return wasBroadcast ?? false;
}
