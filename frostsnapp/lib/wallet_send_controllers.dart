import 'dart:collection';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet_send.dart';

const satoshisInOneBtc = 100000000;

class AddressInputController with ChangeNotifier {
  late final TextEditingController controller;

  String? _errorText;
  String? _lastSubmitted;

  AddressInputController() {
    controller = TextEditingController();
    controller.addListener(onTextEdit);
  }

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  onTextEdit() {
    if (_lastSubmitted == controller.text) return;
    if (_errorText == null) return;
    _errorText = null;
    notifyListeners();
  }

  bool submit(WalletContext walletContext) {
    _lastSubmitted = controller.text;
    _errorText = api.validateDestinationAddressMethodBitcoinNetwork(
        that: walletContext.network, address: controller.text);
    // We always notify listeners on submit (dont' check for changes) for simplicity and safety.
    notifyListeners();
    return _errorText == null;
  }

  String? get errorText => _errorText;

  String? get address => (_errorText == null) ? controller.text : null;

  String get formattedAddress {
    final input = controller.text;
    StringBuffer result = StringBuffer();

    for (int i = 0; i < input.length; i++) {
      result.write(input[i]);

      // Add a space after every 4 characters
      if ((i + 1) % 4 == 0) result.write(' ');
    }

    // Ensure the last group has exactly 4 characters by adding spaces
    int remainder = input.length % 4;
    if (remainder > 0) {
      for (int i = 0; i < 4 - remainder; i++) {
        result.write('\u00A0');
      }
    }
    return result.toString();
  }
}

final defaultTextInputBorder = OutlineInputBorder(
  borderSide: BorderSide.none,
  borderRadius: BorderRadius.circular(8.0),
);

class AddressInput extends StatelessWidget {
  final AddressInputController controller;
  final InputDecoration? decoration;
  final bool autofocus;
  final Function(String)? onSubmitted;

  const AddressInput({
    super.key,
    required this.controller,
    this.decoration,
    this.autofocus = false,
    this.onSubmitted,
  });

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([controller, controller.controller]),
      builder: (context, _) {
        return TextField(
          controller: controller.controller,
          onSubmitted: onSubmitted,
          autofocus: autofocus,
          style: TextStyle(fontFamily: monospaceTextStyle.fontFamily),
          decoration: (decoration ?? InputDecoration()).copyWith(
            //border: defaultTextInputBorder,
            hintText: 'bc1...',
            errorText: controller.errorText,
            suffixIcon: controller.controller.text.isEmpty
                ? null
                : IconButton(
                    onPressed: () => controller.controller.clear(),
                    icon: Icon(Icons.clear),
                  ),
          ),
          keyboardType: TextInputType.text,
          textCapitalization: TextCapitalization.none,
          autocorrect: false,
          enableIMEPersonalizedLearning: false,
          enableSuggestions: false,
          smartQuotesType: SmartQuotesType.disabled,
          smartDashesType: SmartDashesType.disabled,
          minLines: 2,
          maxLines: 4,
        );
      },
    );
  }
}

enum AmountUnit {
  satoshi(
    suffixText: ' sat',
    hintText: '0',
    hasDecimal: false,
  ),
  bitcoin(
    suffixText: ' \u20BF',
    //suffixText: ' bitcoin',
    hintText: '0.0',
    hasDecimal: true,
  );

  const AmountUnit({
    required this.suffixText,
    required this.hintText,
    required this.hasDecimal,
  });

  final String suffixText;
  final String hintText;
  final bool hasDecimal;

  TextInputFormatter get formatter {
    switch (this) {
      case AmountUnit.satoshi:
        return FilteringTextInputFormatter.digitsOnly;
      case AmountUnit.bitcoin:
        return FilteringTextInputFormatter.allow(RegExp(r'^\d*\.?\d*$'));
    }
  }
}

class FeeRateController with ChangeNotifier {
  double _satsPerVB;
  bool _estimateRunning = false;
  Future<bool>? _estimateFut;

  FeeRateController({double satsPerVB = 5.0}) : _satsPerVB = satsPerVB;

  @override
  void dispose() {
    _estimateFut?.ignore();
    super.dispose();
  }

  bool get estimateRunning => _estimateRunning;
  Future<bool> refreshEstimates(BuildContext context,
      WalletContext walletContext, int? setFeeRateToTargetBlocks) async {
    _estimateFut?.ignore();
    _estimateFut =
        _refreshEstimates(context, walletContext, setFeeRateToTargetBlocks);
    return await _estimateFut!;
  }

  Future<bool> _refreshEstimates(BuildContext context,
      WalletContext walletContext, int? setFeeRateToTargetBlocks) async {
    if (context.mounted) {
      _estimateRunning = true;
      notifyListeners();
    } else {
      return false;
    }
    try {
      // Map of feerate(sat/vB) to target blocks.
      var priorityMap = HashMap<int, int>();
      final list = await walletContext.wallet.superWallet
          .estimateFee(targetBlocks: Uint64List.fromList([1, 2, 3, 4, 5, 6]));
      for (final elem in list) {
        final (target, feerate) = elem;
        final oldTarget = priorityMap[feerate];
        if (oldTarget == null || oldTarget > target) {
          priorityMap[feerate] = target;
        }
      }
      if (context.mounted) {
        _priorityMap.clear();
        _priorityMap.addEntries(priorityMap.entries
            .map((e) => MapEntry(e.value, e.key.toDouble())));
        if (setFeeRateToTargetBlocks != null) {
          final feeRateAtTarget = _priorityMap[setFeeRateToTargetBlocks];
          if (feeRateAtTarget != null) {
            _satsPerVB = feeRateAtTarget;
          }
        }
        _estimateRunning = false;
        notifyListeners();
      }
      return true;
    } catch (e) {
      if (context.mounted) {
        _estimateRunning = false;
        notifyListeners();
      }
      return false;
    }
  }

  /// Feerate in sats/vb.
  double get satsPerVB => _satsPerVB;
  set satsPerVB(double value) {
    if (value == _satsPerVB) return;
    _satsPerVB = value;
    notifyListeners();
  }

  /// Map of target blocks to feerate (sats/vb).
  final _priorityMap = SplayTreeMap<int, double>((a, b) => a.compareTo(b));

  Iterable<(int target, double satsPerVB)> get priorityBySatsPerVB =>
      _priorityMap.entries.map((entry) => (entry.key, entry.value));

  set priorityBySatsPerVB(Iterable<(int target, double satsPerVB)> records) {
    _priorityMap.clear();
    for (final record in records) {
      _priorityMap[record.$1] = record.$2;
    }
    notifyListeners();
  }

  set priorityByBtcPerVB(Iterable<(int target, double btcPerVB)> records) {
    _priorityMap.clear();
    for (final record in records) {
      _priorityMap[record.$1] = record.$2 * satoshisInOneBtc;
    }
    notifyListeners();
  }

  int? targetBlocksFromSatsPerVB(double satsPerVB) {
    int? targetBlocks;
    for (final record in priorityBySatsPerVB.toList().reversed) {
      if (record.$2 <= satsPerVB) {
        targetBlocks = record.$1;
      } else {
        break;
      }
    }
    return targetBlocks;
  }

  int? get targetBlocks => targetBlocksFromSatsPerVB(_satsPerVB);

  int? get targetTime {
    final targetBlocks = this.targetBlocks;
    if (targetBlocks == null) return null;
    return targetBlocks * 10;
  }
}

/// Model that tracks avaliable bitcoin.
class AmountAvaliableController extends ValueNotifier<int?> {
  WalletContext? _walletContext;
  List<String> _targetAddresses = [];
  int _targetAmount = 0;

  final FeeRateController feeRateController;

  AmountAvaliableController({required this.feeRateController}) : super(null) {
    onFeeRateChanged();
    feeRateController.addListener(onFeeRateChanged);
  }

  @override
  void dispose() {
    isDisposing = true;
    _calculateAvaliableFut?.ignore();
    feeRateController.removeListener(onFeeRateChanged);
    super.dispose();
  }

  void onFeeRateChanged() => recalculate();

  set walletContext(WalletContext value) {
    _walletContext = value;
    recalculate();
  }

  set targetAddresses(List<String> value) {
    _targetAddresses = value;
    recalculate();
  }

  /// Only use this for more than 1 recipient.
  set targetAmount(int value) {
    _targetAmount = value;
    recalculate();
  }

  bool isDisposing = false;
  Future? _calculateAvaliableFut;

  void recalculate() {
    _calculateAvaliableFut?.ignore();
    _calculateAvaliableFut = _calculateAvaliable();
    _calculateAvaliableFut?.then((maybeNewValue) {
      if (isDisposing || maybeNewValue == null) return;
      var newValue = (maybeNewValue < 0) ? 0 : maybeNewValue;
      if (newValue == value) return;
      value = newValue;
    });
  }

  Future<int?> _calculateAvaliable() async {
    if (_walletContext == null) return null;
    return await _walletContext!.wallet.superWallet.calculateAvaliable(
          masterAppkey: _walletContext!.masterAppkey,
          targetAddresses: _targetAddresses,
          feerate: feeRateController.satsPerVB,
        ) -
        _targetAmount;
  }
}

class AmountInputController with ChangeNotifier {
  AmountInputController({
    required AmountAvaliableController amountAvailableController,
    AmountUnit unit = AmountUnit.satoshi,
    int? amount,
    String? error,
  })  : _unit = unit,
        _amount = amount,
        _textError = error {
    _amountAvailableController = amountAvailableController;
    _amountAvailableController.addListener(onAmountAvailableChanged);

    _textEditingController = TextEditingController();
    _textEditingController
        .addListener(() => amountText = _textEditingController.text);
  }

  @override
  void dispose() {
    _amountAvailableController.removeListener(onAmountAvailableChanged);
    _textEditingController.dispose();
    super.dispose();
  }

  void onAmountAvailableChanged() {
    final avaliableErrorChanged = updateAvailableError();

    // Clear custom error when avaliable amount changes.
    final customErrorChanged = _customError != null;
    _customError = null;

    if (avaliableErrorChanged || customErrorChanged) notifyListeners();
  }

  bool updateAvailableError() {
    String? newAvailableError;
    if (_amountAvailableController.value != null) {
      final amountAvailable = _amountAvailableController.value!;
      if (amountAvailable == 0) {
        newAvailableError = 'No balance avaliable.';
      } else if (_amount != null && amountAvailable < _amount!) {
        newAvailableError = 'Exceeds max by ${_amount! - amountAvailable} sat.';
      }
    }
    final isNew = newAvailableError != _availableError;
    _availableError = newAvailableError;
    return isNew;
  }

  late final AmountAvaliableController _amountAvailableController;

  late final TextEditingController _textEditingController;
  get textEditingController => _textEditingController;

  // The amount unit to use. Currently, bitcoin, satoshis are supported. Fiat support planned.
  AmountUnit _unit;
  AmountUnit get unit => _unit;
  set unit(AmountUnit value) {
    if (value == _unit) return;
    _unit = value;
    _textEditingController.text = amountText;
    notifyListeners();
  }

  // Bitcoin amount in satoshis. The `null` value means user has not yet provided any input.
  int? _amount;
  int? _oldAmount;
  int? get amount => _amount;

  bool get sendMax =>
      _amountAvailableController.value != null &&
      _amountAvailableController.value != 0 &&
      _amount != null &&
      _amount != 0 &&
      _amountAvailableController.value! == _amount!;
  set sendMax(bool value) {
    if (_amountAvailableController.value == null) return;
    if (value) {
      _oldAmount = _amount;
      _amount = _amountAvailableController.value;
      _textEditingController.text = amountText;
      notifyListeners();
    } else {
      _amount = _oldAmount;
      _textEditingController.text = amountText;
      notifyListeners();
    }
  }

  // Error that can be set externally.
  String? _customError;
  set customError(String value) {
    if (value == _customError) return;
    _customError = value;
    notifyListeners();
  }

  // Error string for when user input is not a valid bitcoin/satoshi amount.
  String? _textError;
  // Error string for when amount specified surpases avaliable.
  String? _availableError;
  String? get error {
    if (_textEditingController.text.isEmpty) return null;
    if (_textError != null) return _textError;
    if (_availableError != null) return _availableError;
    if (_customError != null) return _customError;
    return null;
  }

  /// Switch to the next [unit].
  nextUnit() {
    unit = switch (_unit) {
      AmountUnit.satoshi => AmountUnit.bitcoin,
      AmountUnit.bitcoin => AmountUnit.satoshi,
    };
  }

  /// Amount as a text representation.
  String get amountText {
    if (_amount == null) return '';
    switch (_unit) {
      case AmountUnit.satoshi:
        return _amount.toString();
      case AmountUnit.bitcoin:
        return (_amount! / satoshisInOneBtc)
            .toStringAsFixed(8)
            // Remove trailing zeros and/or a trailing dot, but preserve significant digits
            .replaceAllMapped(
              RegExp(r'(\.[0-9]*?[1-9])0+$'),
              (match) => match.group(1)!,
            );
    }
  }

  /// Update state by applying an [text] from user input.
  set amountText(String text) {
    int? newAmount;
    String? newError;

    switch (_unit) {
      case AmountUnit.satoshi:
        final parsedAmount = int.tryParse(text);
        if (parsedAmount == null) {
          newAmount = _amount;
          newError = 'Invalid satoshi amount.';
        } else {
          newAmount = parsedAmount;
          newError = null;
        }
      case AmountUnit.bitcoin:
        final parsedAmount = double.tryParse(text);
        if (parsedAmount == null) {
          newAmount = _amount;
          newError = 'Invalid bitcoin amount.';
        } else if (getDecimalPlaceCount(parsedAmount) > 8) {
          newAmount = _amount;
          newError = 'Too many decimal places.';
        } else {
          newAmount = (parsedAmount * satoshisInOneBtc).round();
          newError = null;
        }
    }

    var isChanged = newAmount != _amount || newError != _textError;
    _amount = newAmount;
    _textError = newError;
    _customError = null;
    var isAvaliableErrorChanged = updateAvailableError();
    if (isChanged || isAvaliableErrorChanged) notifyListeners();
  }
}

int getDecimalPlaceCount(double number) {
  // Convert the double to a string
  String numberString = number.toString();

  // Check if the string contains a decimal point
  if (numberString.contains('.')) {
    return numberString
        .split('.')[1]
        .length; // Return the length of the fractional part
  }

  // No decimal places
  return 0;
}

class AmountInput extends StatelessWidget {
  final AmountInputController model;
  final void Function(String)? onSubmitted;
  final InputDecoration? decoration;

  const AmountInput({
    super.key,
    required this.model,
    this.decoration,
    this.onSubmitted,
  });

  onUnitButtonPressed() => model.nextUnit();

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: model,
      builder: (context, child) => TextField(
        controller: model.textEditingController,
        style: TextStyle(fontFamily: monospaceTextStyle.fontFamily),
        decoration: (decoration ?? InputDecoration()).copyWith(
          errorText: model.error,
          hintText: model.unit.hintText,
          suffixText: model.unit.suffixText,
          suffixIcon: IconButton(
            onPressed: onUnitButtonPressed,
            icon: Icon(Icons.swap_vert),
          ),
          border: defaultTextInputBorder,
        ),
        keyboardType: TextInputType.numberWithOptions(
          signed: false,
          decimal: model.unit.hasDecimal,
        ),
        inputFormatters: [model.unit.formatter],
        autofocus: true,
        onSubmitted: onSubmitted,
      ),
    );
  }
}

class DeviceModel {
  final DeviceId id;
  final bool selected;
  late final String? name;
  late final int nonces;
  late final bool canSelect;

  DeviceModel({
    required this.id,
    required this.selected,
    required bool thresholdMet,
  }) {
    name = coord.getDeviceName(id: id);
    nonces = coord.noncesAvailable(id: id);
    canSelect = nonces > 0 && (!thresholdMet || selected);
  }

  bool get enoughNonces => nonces >= 1;
}

class SelectedDevicesController with ChangeNotifier {
  static const accessStructureIndex = 0;

  WalletContext? _walletContext;
  FrostKey? _frostKey;

  final HashSet<DeviceId> _selected = HashSet(
    equals: (a, b) => a.field0.toString() == b.field0.toString(),
    hashCode: (id) => id.field0.toString().hashCode,
  );

  SelectedDevicesController();

  set walletContext(WalletContext walletContext) {
    _walletContext = walletContext;
    _frostKey = coord.getFrostKey(keyId: walletContext.keyId)!;
    notifyListeners();
  }

  Set<DeviceId> get selected => _selected;

  Iterable<DeviceModel> get devices {
    final isThresholdMet = this.isThresholdMet;
    if (_frostKey == null) return [];
    return _frostKey!
        .accessStructures()[accessStructureIndex]
        .devices()
        .map((id) => DeviceModel(
              id: id,
              selected: _selected.contains(id),
              thresholdMet: isThresholdMet,
            ));
  }

  int get threshold => (_frostKey == null)
      ? 0
      : _frostKey!.accessStructures()[accessStructureIndex].threshold();
  bool get isThresholdMet => _frostKey != null && _selected.length >= threshold;
  int get remaining => threshold - _selected.length;

  void select(DeviceId id) => _selected.add(id) ? notifyListeners() : null;
  void deselect(DeviceId id) => _selected.remove(id) ? notifyListeners() : null;

  void signAndBroadcast(
      BuildContext context, UnsignedTx unsignedTx, VoidCallback? onBroadcast) {
    if (_walletContext == null || _frostKey == null) return;
    final accessStructure = _frostKey!.accessStructures()[accessStructureIndex];
    final signingStream = coord.startSigningTx(
      accessStructureRef: accessStructure.accessStructureRef(),
      unsignedTx: unsignedTx,
      devices: _selected.toList(),
    );
    if (context.mounted) {
      signAndBroadcastWorkflowDialog(
          context: context,
          signingStream: signingStream,
          unsignedTx: unsignedTx,
          superWallet: _walletContext!.wallet.superWallet,
          masterAppkey: _walletContext!.masterAppkey,
          onBroadcastNewTx: onBroadcast);
    }
  }
}
