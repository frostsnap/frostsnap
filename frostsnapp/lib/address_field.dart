import 'dart:collection';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/wallet_send.dart';

const satoshisInOneBtc = 100000000;

class AddressModel with ChangeNotifier {
  late final TextEditingController controller;

  String? _errorText;
  String? _lastSubmitted;

  AddressModel() {
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
        that: walletContext.wallet.network, address: controller.text);
    // We always notify listeners on submit (dont' check for changes) for simplicity and safety.
    notifyListeners();
    return _errorText == null;
  }

  String? get errorText => _errorText;

  String? get address => (_errorText == null) ? controller.text : null;

  String get formattedAddress {
    final input = controller.text;
    StringBuffer result = StringBuffer();

    //int groupCount = 0; // Track the number of groups of 4
    for (int i = 0; i < input.length; i++) {
      result.write(input[i]);

      // Add a space after every 4 characters
      if ((i + 1) % 4 == 0) {
        //groupCount++;

        //// Add a new line after every 4 groups of 4 characters
        //if (groupCount % 4 == 0) {
        //  result.write('\n');
        //} else {
        result.write(' ');
        //}
      }
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

class AddressField extends StatelessWidget {
  final AddressModel model;
  final InputDecoration? decoration;
  final bool autofocus;
  final Function(String)? onSubmitted;

  const AddressField({
    super.key,
    required this.model,
    this.decoration,
    this.autofocus = false,
    this.onSubmitted,
  });

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([model, model.controller]),
      builder: (context, _) {
        return TextField(
          controller: model.controller,
          onSubmitted: onSubmitted,
          autofocus: autofocus,
          style: TextStyle(fontFamily: addressTextStyle.fontFamily),
          decoration: (decoration ?? InputDecoration()).copyWith(
            border: defaultTextInputBorder,
            hintText: 'bc1...',
            errorText: model.errorText,
            suffixIcon: model.controller.text.isEmpty
                ? null
                : IconButton(
                    onPressed: () => model.controller.clear(),
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
          minLines: 1,
          maxLines: 4,
        );
      },
    );
  }
}

enum AmountUnit {
  satoshi(
    suffixText: ' sats',
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

enum Priority {
  unknown(name: 'Unknown'),
  oneBlock(name: 'High', targetBlocks: 1),
  twoBlocks(name: 'Medium', targetBlocks: 2),
  threeBlocks(name: 'Low', targetBlocks: 3),
  fourBlocks(name: 'Very Low', targetBlocks: 4),
  fiveBlocks(name: 'Extremely Low', targetBlocks: 5),
  sixBlocksOrMore(name: 'Tremendously Low', targetBlocks: 6);

  const Priority({required this.name, this.targetBlocks});

  static Priority fromTarget(int target) {
    switch (target) {
      case 1:
        return Priority.oneBlock;
      case 2:
        return Priority.twoBlocks;
      case 3:
        return Priority.threeBlocks;
      case 4:
        return Priority.fourBlocks;
      case 5:
        return Priority.fiveBlocks;
      case 6:
        return Priority.sixBlocksOrMore;
      case _:
        return Priority.unknown;
    }
  }

  final String name;
  final int? targetBlocks;
  int? get targetTime => (targetBlocks == null) ? null : targetBlocks! * 10;
}

class FeeRateModel with ChangeNotifier {
  final int maxTargetBlocks;

  FeeRateModel({
    double satsPerVB = 5.0,
    this.maxTargetBlocks = 12,
  }) : _satsPerVB = satsPerVB;

  bool _estimateRunning = false;
  bool get estimateRunning => _estimateRunning;
  Future<bool> refreshEstimates(BuildContext context,
      WalletContext walletContext, int? setFeeRateToTargetBlocks) async {
    if (!context.mounted) return false;

    if (context.mounted) {
      _estimateRunning = true;
      notifyListeners();
    } else {
      return false;
    }

    // Map of feerate(sat/vB) to target blocks.
    var priorityMap = HashMap<int, int>();

    try {
      final list = await walletContext.wallet
          .estimateFee(targetBlocks: Uint64List.fromList([1, 2, 3, 4, 5, 6]));
      for (final elem in list) {
        final (target, feerate) = elem;
        final oldTarget = priorityMap[feerate];
        if (oldTarget == null || oldTarget > target) {
          priorityMap[feerate] = target;
        }
      }
    } catch (e) {
      if (context.mounted) {
        _estimateRunning = false;
        notifyListeners();
      }
      return false;
    }

    if (context.mounted) {
      _priorityMap.clear();
      _priorityMap.addEntries(
          priorityMap.entries.map((e) => MapEntry(e.value, e.key.toDouble())));
      _estimateRunning = false;
      //TODO
      if (setFeeRateToTargetBlocks != null) {
        final feeRateAtTarget = _priorityMap[setFeeRateToTargetBlocks];
        if (feeRateAtTarget != null) {
          _satsPerVB = feeRateAtTarget;
        }
      }
      notifyListeners();
    }
    return true;
  }

  /// Feerate in sats/vb.
  double _satsPerVB;
  double get satsPerVB => _satsPerVB;
  double get satsPerWU => (_satsPerVB / 4);
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
class AmountAvaliableModel extends ValueNotifier<int?> {
  WalletContext? _walletContext;
  List<String> _targetAddresses = [];
  int _targetAmount = 0;

  final FeeRateModel feeRateModel;

  AmountAvaliableModel({required this.feeRateModel}) : super(null) {
    onFeeRateChanged();
    feeRateModel.addListener(onFeeRateChanged);
  }

  @override
  void dispose() {
    feeRateModel.removeListener(onFeeRateChanged);
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

  void recalculate() {
    _calculateAvaliable().then((maybeNewValue) {
      if (maybeNewValue == null) return;
      var newValue = maybeNewValue;
      if (newValue < 0) newValue = 0;
      if (newValue == value) return;
      value = newValue;
    });
  }

  Future<int?> _calculateAvaliable() async {
    if (_walletContext == null) return null;
    return await _walletContext!.wallet.calculateAvaliable(
          masterAppkey: _walletContext!.masterAppkey,
          targetAddresses: _targetAddresses,
          feerate: feeRateModel.satsPerVB,
        ) -
        _targetAmount;
  }
}

class AmountInputModel with ChangeNotifier {
  AmountInputModel({
    required AmountAvaliableModel avaliableAmountModel,
    AmountUnit unit = AmountUnit.satoshi,
    int? amount,
    String? error,
  })  : _unit = unit,
        _amount = amount,
        _textError = error {
    _avaliableAmountModel = avaliableAmountModel;
    _avaliableAmountModel.addListener(onAvaliableAmountChanged);

    _textEditingController = TextEditingController();
    _textEditingController
        .addListener(() => amountText = _textEditingController.text);
  }

  @override
  void dispose() {
    _avaliableAmountModel.removeListener(onAvaliableAmountChanged);
    _textEditingController.dispose();
    super.dispose();
  }

  void onAvaliableAmountChanged() {
    final avaliableErrorChanged = updateAvaliableError();

    // Clear custom error when avaliable amount changes.
    final customErrorChanged = _customError != null;
    _customError = null;

    if (avaliableErrorChanged || customErrorChanged) notifyListeners();
  }

  bool updateAvaliableError() {
    String? newAvaliableError;
    if (_avaliableAmountModel.value != null) {
      if (_avaliableAmountModel.value! == 0) {
        newAvaliableError = 'No balance avaliable.';
      } else if (_amount != null && _avaliableAmountModel.value! < _amount!) {
        newAvaliableError = 'Exceeds max.';
      }
    }
    final isNew = newAvaliableError != _avaliableError;
    _avaliableError = newAvaliableError;
    return isNew;
  }

  late final AmountAvaliableModel _avaliableAmountModel;

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

  bool get sendAll =>
      _avaliableAmountModel.value != null &&
      _avaliableAmountModel.value != 0 &&
      _amount != null &&
      _amount != 0 &&
      _avaliableAmountModel.value! == _amount!;
  set sendAll(bool value) {
    if (_avaliableAmountModel.value == null) return;
    if (value) {
      _oldAmount = _amount;
      _amount = _avaliableAmountModel.value;
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
  String? _avaliableError;
  String? get error {
    if (_textEditingController.text.isEmpty) return null;
    if (_textError != null) return _textError;
    if (_avaliableError != null) return _avaliableError;
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
    var isAvaliableErrorChanged = updateAvaliableError();
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

class AmountField extends StatelessWidget {
  final AmountInputModel model;
  final void Function(String)? onSubmitted;
  final InputDecoration? decoration;

  const AmountField({
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
        style: TextStyle(fontFamily: balanceTextStyle.fontFamily),
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

class SelectedDevicesModel with ChangeNotifier {
  static const accessStructureIndex = 0;

  WalletContext? _walletContext;
  FrostKey? _frostKey;

  final HashSet<DeviceId> _selected = HashSet(
    equals: (a, b) => a.field0.toString() == b.field0.toString(),
    hashCode: (id) => id.field0.toString().hashCode,
  );

  SelectedDevicesModel();

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
          wallet: _walletContext!.wallet,
          context: context,
          signingStream: signingStream,
          unsignedTx: unsignedTx,
          masterAppkey: _walletContext!.masterAppkey,
          onBroadcastNewTx: onBroadcast);
    }
  }
}
