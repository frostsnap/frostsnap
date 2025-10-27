import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/broadcast.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';

enum Eta {
  low,
  medium,
  high;

  int get targetBlocks => switch (this) {
    Eta.low => 3,
    Eta.medium => 2,
    Eta.high => 1,
  };
}

class FeeRatePickerDialog extends StatefulWidget {
  final BuildTxState state;
  final WalletContext walletContext;

  const FeeRatePickerDialog({
    super.key,
    required this.walletContext,
    required this.state,
  });

  @override
  State<FeeRatePickerDialog> createState() => _FeeRatePickerDialogState();
}

class _FeeRatePickerDialogState extends State<FeeRatePickerDialog> {
  BuildTxState get state => widget.state;
  // Subscription to `BuildTxState` changes.
  late final UnitBroadcastSubscription sub;
  // Current selection.
  Eta? currentSelection;
  // Custom feerate input controller.
  late final TextEditingController customFeerateController;
  String? customFeerateError;

  @override
  initState() {
    super.initState();

    sub = widget.state.subscribe();
    sub.start().listen((_) => mounted ? setState(() {}) : null);

    final currentTarget = state.confirmationTarget();

    currentSelection = switch (currentTarget) {
      ConfirmationTarget_Low() => Eta.low,
      ConfirmationTarget_Medium() => Eta.medium,
      ConfirmationTarget_High() => Eta.high,
      ConfirmationTarget_Custom() => null,
    };
    customFeerateController = TextEditingController(
      text: switch (currentTarget) {
        ConfirmationTarget_Custom(:final field0) => field0.toStringAsFixed(1),
        _ => (1.0).toStringAsFixed(1),
      },
    );
  }

  @override
  void dispose() {
    customFeerateController.dispose();
    sub.dispose();
    super.dispose();
  }

  _onRefresh(BuildContext context) async {
    final _ = await state.refreshConfirmationEstimates();
  }

  void _onTapTile(BuildContext context, Eta? eta) {
    setState(() => currentSelection = eta);

    if (eta == null) return;

    final target = switch (eta) {
      Eta.low => ConfirmationTarget.low(),
      Eta.medium => ConfirmationTarget.medium(),
      Eta.high => ConfirmationTarget.high(),
    };

    state.setConfirmationTarget(target: target);
    Navigator.pop(context, target);
  }

  void _onSubmitCustomFeerate(BuildContext, String text) {
    final feerate = double.tryParse(text);
    if (feerate == null) {
      setState(() => customFeerateError = 'Invalid feerate');
      return;
    }

    final target = ConfirmationTarget.custom(feerate);
    state.setConfirmationTarget(target: target);
    Navigator.pop(context, target);
  }

  Widget _buildEtaTile(BuildContext context, {Eta? eta}) {
    final theme = Theme.of(context);
    final estimates = state.confirmationEstimates();

    final isSelected = currentSelection == eta;

    final Widget leadingIcon = Icon(
      isSelected ? Icons.radio_button_on : Icons.radio_button_off,
    );

    final Widget timeText = Text.rich(
      TextSpan(
        children: [
          TextSpan(text: eta != null ? '${eta.targetBlocks * 10} ' : 'Custom'),
          if (eta != null)
            TextSpan(
              text: 'min ETA',
              style: TextStyle(fontSize: theme.textTheme.labelMedium!.fontSize),
            ),
        ],
      ),
    );

    final Widget feerateTextOrInput;
    if (eta != null) {
      final estimateFeerate =
          switch (eta) {
            Eta.low => estimates?.low,
            Eta.medium => estimates?.medium,
            Eta.high => estimates?.high,
          } ??
          '~';
      feerateTextOrInput = Text.rich(
        TextSpan(
          children: [
            TextSpan(text: '$estimateFeerate '),
            TextSpan(
              text: ' sat/vB',
              style: TextStyle(fontSize: theme.textTheme.labelMedium!.fontSize),
            ),
          ],
        ),
      );
    } else {
      feerateTextOrInput = SizedBox(
        width: 150,
        child: TextField(
          controller: customFeerateController,
          // Highlight on tap.
          onTap: () => customFeerateController.selection = TextSelection(
            baseOffset: 0,
            extentOffset: customFeerateController.text.length,
          ),
          // Reset error when user is actively changing the value.
          onChanged: (_) {
            if (customFeerateError != null)
              setState(() => customFeerateError = null);
          },
          onSubmitted: (text) => _onSubmitCustomFeerate(context, text),
          enabled: isSelected,
          decoration: InputDecoration(
            suffixIcon: IconButton(
              icon: Icon(Icons.done),
              onPressed: () =>
                  _onSubmitCustomFeerate(context, customFeerateController.text),
            ),
            suffixText: 'sat/vB',
            suffixStyle: theme.textTheme.labelMedium,
            border: OutlineInputBorder(),
            errorText: customFeerateError,
          ),
          keyboardType: TextInputType.numberWithOptions(
            signed: false,
            decimal: true,
          ),
          inputFormatters: [
            FilteringTextInputFormatter.allow(RegExp(r'^\d*\.?\d*$')),
          ],
        ),
      );
    }

    return ListTile(
      enabled: eta == null || estimates != null,
      onTap: () => _onTapTile(context, eta),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(16)),
      ),
      selected: isSelected,
      leading: leadingIcon,
      title: timeText,
      trailing: feerateTextOrInput,
      leadingAndTrailingTextStyle: theme.textTheme.titleMedium,
      contentPadding: EdgeInsets.symmetric(vertical: 8, horizontal: 16),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final feerateTitle = Padding(
      padding: EdgeInsetsGeometry.fromLTRB(16, 16, 16, 0),
      child: Text('Feerate', style: theme.textTheme.titleLarge),
    );

    final pullDownToRefresh = InkWell(
      customBorder: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(4.0),
      ),
      onTap: () => _onRefresh(context),
      child: SizedBox(
        height: 32.0,
        width: 192.0,
        child: Center(
          child: AnimatedCrossFade(
            firstChild: LinearProgressIndicator(
              borderRadius: BorderRadius.circular(4.0),
            ),
            secondChild: Text(
              'Pull down or tap to refresh.',
              softWrap: true,
              style: theme.textTheme.labelSmall,
            ),
            crossFadeState: state.isRefreshingConfirmationEstimates()
                ? CrossFadeState.showFirst
                : CrossFadeState.showSecond,
            duration: Durations.medium2,
          ),
        ),
      ),
    );

    final etaListCard = Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Column(
        children: [
          _buildEtaTile(context, eta: Eta.high),
          _buildEtaTile(context, eta: Eta.medium),
          _buildEtaTile(context, eta: Eta.low),
          _buildEtaTile(context, eta: null),
        ],
      ),
    );

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
              children: [feerateTitle, pullDownToRefresh, etaListCard],
            ),
          ),
        ),
      ),
    );
  }
}
