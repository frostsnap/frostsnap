import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/wallet_send_controllers.dart';

enum FeeRatePage {
  eta,
  feerate,
}

class FeeRatePickerDialog extends StatefulWidget {
  final WalletContext walletContext;
  final AddressInputController addressModel;
  final FeeRateController feeRateModel;
  final AmountInputController amountModel;

  const FeeRatePickerDialog({
    super.key,
    required this.walletContext,
    required this.addressModel,
    required this.feeRateModel,
    required this.amountModel,
  });

  @override
  State<FeeRatePickerDialog> createState() => _FeeRatePickerDialogState();
}

class _FeeRatePickerDialogState extends State<FeeRatePickerDialog> {
  late final TextEditingController _feeRateEditingController;
  String? _feeRateEditingError;

  int? _feeAmount;
  String? _feeAmountError;

  @override
  initState() {
    super.initState();

    final satsPerVB = widget.feeRateModel.satsPerVB;
    _feeRateEditingController =
        TextEditingController(text: satsPerVB.toString());
    _feeRateEditingController.addListener(_onChangedFeeRateInput);
    _tryCalculateFee();
  }

  @override
  void dispose() {
    _feeRateEditingController.dispose();
    super.dispose();
  }

  _onTapEtaTile(double tileSatsPerVB) {
    final newText = tileSatsPerVB.toString();
    _feeRateEditingController.text = newText;
  }

  _onRefresh(BuildContext context) async {
    await widget.feeRateModel
        .refreshEstimates(context, widget.walletContext, null);
  }

  _onTapSubmitButton() {
    if (_feeRateEditingError != null) return;
    Navigator.pop(context);
  }

  _onChangedFeeRateInput() {
    final double newSatsPerVB;
    try {
      newSatsPerVB = double.parse(_feeRateEditingController.text);
    } catch (e) {
      setState(() => _feeRateEditingError = e.toString());
      return;
    }
    if (newSatsPerVB == widget.feeRateModel.satsPerVB &&
        _feeRateEditingError == null) {
      return;
    }
    setState(() => _feeRateEditingError = null);
    widget.feeRateModel.satsPerVB = newSatsPerVB;
    _tryCalculateFee();
  }

  _tryCalculateFee() async {
    if (!context.mounted) return;
    setState(() {
      _feeAmountError = null;
      _feeAmount = null;
    });

    final walletCtx = widget.walletContext;
    final masterAppkey = walletCtx.masterAppkey;

    if (widget.amountModel.amount == null) {
      setState(() => _feeAmountError = 'Cannot calculate fee: no send amount.');
      return;
    }
    final amount = widget.amountModel.amount!;
    final feeRate = widget.feeRateModel.satsPerVB;
    late final String address;

    // Tru get address
    if (widget.addressModel.address == null) {
      setState(() => _feeAmountError = 'Cannot calculate fee: no recipient.');
    }
    address = widget.addressModel.address!;

    try {
      final tx = await walletCtx.wallet.superWallet.sendTo(
        masterAppkey: masterAppkey,
        toAddress: address,
        value: amount,
        feerate: feeRate,
      );
      final fee = tx.fee();
      if (context.mounted) setState(() => _feeAmount = fee);
    } catch (e) {
      if (context.mounted) {
        setState(() => _feeAmountError = e
            .toString()
            .replaceAll('FrbAnyhowException(', '')
            .replaceAll(')', ''));
      }
    }
  }

  Iterable<Widget> _buildEtaTiles() {
    final theme = Theme.of(context);

    final selectedTargetBlocks = widget.feeRateModel.targetBlocks;

    final tiles = widget.feeRateModel.priorityBySatsPerVB.map<Widget>((record) {
      final targetBlocks = record.$1;
      final satsPerVB = record.$2;
      final isPrioritySameAsSelected = selectedTargetBlocks == targetBlocks;
      final isSelected = widget.feeRateModel.satsPerVB == satsPerVB;

      final feeRateText = Text.rich(
        TextSpan(
          children: [
            TextSpan(text: '$satsPerVB '),
            TextSpan(
              text: 'sat/vB',
              style: TextStyle(fontSize: theme.textTheme.labelMedium!.fontSize),
            ),
          ],
        ),
      );

      final timeText = Text.rich(
        TextSpan(
          children: [
            TextSpan(text: '${targetBlocks * 10} '),
            TextSpan(
              text: 'min ETA',
              style: TextStyle(fontSize: theme.textTheme.labelMedium!.fontSize),
            ),
          ],
        ),
      );

      final leadingIcon = Icon(
        isSelected
            ? Icons.radio_button_on
            : isPrioritySameAsSelected
                ? Icons.indeterminate_check_box_outlined
                : Icons.radio_button_off,
        key: UniqueKey(),
      );

      final tile = ListTile(
        onTap: () => _onTapEtaTile(satsPerVB),
        selected: isSelected,
        leading: leadingIcon,
        title: timeText,
        trailing: feeRateText,
        leadingAndTrailingTextStyle: theme.textTheme.titleMedium,
      );
      return tile;
    });

    return tiles;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final etaListCard = Card.filled(
      margin: EdgeInsets.all(0.0),
      color: theme.colorScheme.surfaceContainerHigh,
      child: ListenableBuilder(
          listenable: widget.feeRateModel,
          builder: (context, _) =>
              Column(children: _buildEtaTiles().toList().reversed.toList())),
    );

    final feeRateField = TextField(
      controller: _feeRateEditingController,
      // Highlight on tap.
      onTap: () => _feeRateEditingController.selection = TextSelection(
        baseOffset: 0,
        extentOffset: _feeRateEditingController.text.length,
      ),
      decoration: InputDecoration(
        labelText: 'Fee Rate',
        prefixIcon: Icon(Icons.edit_rounded),
        suffixText: 'sat/vB',
        suffixStyle: theme.textTheme.labelLarge,
        errorText: _feeRateEditingError,
        helperText: _feeAmountError ?? 'Fee Amount: $_feeAmount sats',
        border: OutlineInputBorder(borderSide: BorderSide.none),
      ),
      keyboardType: TextInputType.numberWithOptions(
        signed: false,
        decimal: true,
      ),
      inputFormatters: [
        FilteringTextInputFormatter.allow(RegExp(r'^\d*\.?\d*$')),
      ],
    );

    final submitButton = IconButton.filled(
      onPressed:
          _feeRateEditingError == null ? () => _onTapSubmitButton() : null,
      icon: Icon(Icons.done),
      style: IconButton.styleFrom(
        elevation: 3.0,
        shadowColor: theme.colorScheme.shadow,
      ),
    );

    final pullDownToRefresh = InkWell(
      customBorder:
          RoundedRectangleBorder(borderRadius: BorderRadius.circular(4.0)),
      onTap: () => _onRefresh(context),
      child: SizedBox(
        height: 32.0,
        width: 192.0,
        child: Center(
          child: ListenableBuilder(
            listenable: widget.feeRateModel,
            builder: (context, _) => AnimatedCrossFade(
              firstChild: LinearProgressIndicator(
                borderRadius: BorderRadius.circular(4.0),
              ),
              secondChild: Text(
                'Pull down or tap to refresh.',
                softWrap: true,
                style: theme.textTheme.labelSmall,
              ),
              crossFadeState: widget.feeRateModel.estimateRunning
                  ? CrossFadeState.showFirst
                  : CrossFadeState.showSecond,
              duration: Durations.medium2,
            ),
          ),
        ),
      ),
    );

    final feeRateCard = Card(
      margin: EdgeInsets.all(0.0),
      color: theme.colorScheme.surfaceContainerHigh,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12.0),
      ),
      child: Padding(
        padding: EdgeInsets.fromLTRB(8.0, 24.0, 8.0, 16.0),
        child: feeRateField,
      ),
    );

    final columnWidgets = [
      pullDownToRefresh,
      etaListCard,
      feeRateCard,
      Row(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [submitButton],
      )
    ];

    return Dialog(
      backgroundColor: Colors.transparent,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: 580),
        child: RefreshIndicator(
          onRefresh: () async => _onRefresh(context),
          child: SingleChildScrollView(
            physics: AlwaysScrollableScrollPhysics(),
            child: Column(
              spacing: 12.0,
              mainAxisSize: MainAxisSize.min,
              children: columnWidgets,
            ),
          ),
        ),
      ),
    );
  }
}
