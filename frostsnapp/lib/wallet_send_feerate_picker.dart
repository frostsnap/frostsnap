import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/broadcast.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';

enum FeeratePage { eta, feerate }

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
  // Subscription to `BuildTxState` changes.
  late final UnitBroadcastSubscription sub;
  // Previous state - in case caller cancels this dialog.
  late final ConfirmationTarget prevConfirmationTarget;
  // Custom feerate controller.
  late final TextEditingController customFeerateController;

  BuildTxState get state => widget.state;

  @override
  initState() {
    super.initState();

    sub = widget.state.subscribe();
    sub.start().listen((_) => mounted ? setState(() {}) : null);

    prevConfirmationTarget = widget.state.confirmationTarget();
    final customFeerateStr = switch (prevConfirmationTarget) {
      ConfirmationTarget_Custom(:final field0) => field0.toStringAsFixed(2),
      _ => (1.0).toStringAsFixed(2),
    };
    customFeerateController = TextEditingController(text: customFeerateStr);
  }

  @override
  void dispose() {
    sub.dispose();
    super.dispose();
  }

  _onRefresh(BuildContext context) async {
    final _ = await state.refreshConfirmationEstimates();
  }

  void _onTapTile(BuildContext context, ConfirmationTarget target) {
    state.setConfirmationTarget(target: target);
  }

  // Only allow submission if form is valid.
  bool _canSubmit() {
    final target = state.confirmationTarget();
    return switch (target) {
      ConfirmationTarget_Custom() =>
        double.tryParse(customFeerateController.text) != null,
      _ => state.confirmationEstimates() != null,
    };
  }

  void _onSubmit(BuildContext context) {
    Navigator.pop(context);
  }

  double? _parseCustomFeerate() {
    final feerate = double.tryParse(customFeerateController.text);
    return feerate;
  }

  Iterable<Widget> _buildEtaTiles(BuildContext context) {
    final current = state.confirmationTarget();

    return [
      _buildEtaTile(context, eta: Eta.high, isSelected: current.isHigh()),
      _buildEtaTile(context, eta: Eta.medium, isSelected: current.isMedium()),
      _buildEtaTile(context, eta: Eta.low, isSelected: current.isLow()),
      _buildEtaTile(context, eta: null, isSelected: current.isCustom()),
    ];
  }

  Widget _buildEtaTile(
    BuildContext context, {
    bool isSelected = false,
    Eta? eta,
  }) {
    final theme = Theme.of(context);

    final Widget leadingIcon = Icon(
      isSelected ? Icons.radio_button_on : Icons.radio_button_off,
    );

    final Widget timeText = Text.rich(
      TextSpan(
        children: [
          TextSpan(text: eta != null ? '${eta.targetBlocks * 10} ' : 'Custom '),
          TextSpan(
            text: '${eta != null ? 'min' : ''}ETA',
            style: TextStyle(fontSize: theme.textTheme.labelMedium!.fontSize),
          ),
        ],
      ),
    );

    final Widget feerateTextOrInput;
    if (eta != null) {
      final estimates = state.confirmationEstimates();
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
        width: 200,
        child: TextField(
          controller: customFeerateController,
          // Highlight on tap.
          onTap: () => customFeerateController.selection = TextSelection(
            baseOffset: 0,
            extentOffset: customFeerateController.text.length,
          ),
          onChanged: (v) => _onTapTile(
            context,
            ConfirmationTarget.custom(_parseCustomFeerate() ?? 1.0),
          ),
          enabled: isSelected,
          autofocus: true,
          decoration: InputDecoration(
            suffixText: 'sat/vB',
            suffixStyle: theme.textTheme.labelLarge,
            border: OutlineInputBorder(),
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
      onTap: () => _onTapTile(context, switch (eta) {
        null => ConfirmationTarget.custom(_parseCustomFeerate() ?? 1.0),
        Eta.low => ConfirmationTarget.low(),
        Eta.medium => ConfirmationTarget.medium(),
        Eta.high => ConfirmationTarget.high(),
      }),
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

    final etaListCard = Card.filled(
      margin: EdgeInsets.all(0.0),
      color: theme.colorScheme.surfaceContainerHigh,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Column(
          children: _buildEtaTiles(context).toList(growable: false),
        ),
      ),
    );

    final submitButton = IconButton.filled(
      onPressed: _canSubmit() ? () => _onSubmit(context) : null,
      icon: Icon(Icons.done),
      style: IconButton.styleFrom(
        elevation: 3.0,
        shadowColor: theme.colorScheme.shadow,
      ),
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

    final columnWidgets = [
      pullDownToRefresh,
      etaListCard,
      Row(mainAxisAlignment: MainAxisAlignment.end, children: [submitButton]),
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
