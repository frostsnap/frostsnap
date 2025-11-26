import 'dart:async';
import 'dart:collection';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';
import 'package:frostsnap/theme.dart';

const satoshisInOneBtc = 100000000;

class AddressInputController with ChangeNotifier {
  late final TextEditingController controller;
  late final BuildTxState state;

  String? _errorText;
  String? _lastSubmitted;

  AddressInputController(BuildTxState state) {
    controller = TextEditingController();
    controller.addListener(onTextEdit);
    this.state = state;
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

  bool submit(int recipient) {
    // TODO: There is the URI thing we're not doing here.
    _lastSubmitted = controller.text;

    try {
      state.setRecipientWithUri(recipient: recipient, uri: controller.text);
    } on String catch (e) {
      _errorText = e;
      return false;
    }

    // We always notify listeners on submit (dont' check for changes) for simplicity and safety.
    notifyListeners();
    return true;
  }

  String? get errorText => _errorText;
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
          minLines: 1,
          maxLines: 4,
        );
      },
    );
  }
}

enum AmountUnit {
  satoshi(suffixText: ' sat', hintText: '0', hasDecimal: false),
  bitcoin(suffixText: ' \u20BF', hintText: '0.0', hasDecimal: true);

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

class AmountInputController with ChangeNotifier {
  final BuildTxState state;

  AmountInputController({
    required BuildTxState state,
    AmountUnit unit = AmountUnit.satoshi,
    int? amount,
    String? error,
  }) : _unit = unit,
       _amount = amount,
       _textError = error,
       this.state = state {
    _textEditingController = TextEditingController();
    _textEditingController.addListener(
      () => amountText = _textEditingController.text,
    );
  }

  @override
  void dispose() {
    _textEditingController.dispose();
    super.dispose();
  }

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
  int? get amount => _amount;
  set amount(int? v) {
    _amount = v;
    _textEditingController.text = amountText;
    notifyListeners();
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

    if (newAmount != null) {
      state.setAmount(recipient: 0, amount: newAmount);
    } else {
      state.clearAmount(recipient: 0);
    }

    var isChanged = newError != _textError;
    _textError = newError;
    _customError = null;
    if (isChanged) notifyListeners();
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

  final HashSet<DeviceId> _selected = deviceIdSet([]);

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
    return _frostKey!.accessStructures()[accessStructureIndex].devices().map(
      (id) => DeviceModel(
        id: id,
        selected: _selected.contains(id),
        thresholdMet: isThresholdMet,
      ),
    );
  }

  Iterable<DeviceModel> get selectedDevices =>
      devices.where((device) => device.selected);

  int get threshold => (_frostKey == null)
      ? 0
      : _frostKey!.accessStructures()[accessStructureIndex].threshold();
  bool get isThresholdMet => _frostKey != null && _selected.length >= threshold;
  int get remaining => threshold - _selected.length;

  void select(DeviceId id) => _selected.add(id) ? notifyListeners() : null;
  void deselect(DeviceId id) => _selected.remove(id) ? notifyListeners() : null;

  Stream<SigningState>? signingSessionStream(UnsignedTx unsignedTx) {
    if (_walletContext == null || _frostKey == null) return null;
    final accessStructure = _frostKey!.accessStructures()[accessStructureIndex];
    return coord.startSigningTx(
      accessStructureRef: accessStructure.accessStructureRef(),
      unsignedTx: unsignedTx,
      devices: _selected.toList(),
    );
  }
}

class DeviceSignatureModel {
  final DeviceId id;
  final bool hasSignature;
  final bool isConnected;
  late final String? name;

  DeviceSignatureModel({
    required this.id,
    required this.hasSignature,
    required this.isConnected,
  }) {
    name = coord.getDeviceName(id: id);
  }
}

class SigningSessionController with ChangeNotifier {
  late final StreamSubscription<DeviceListUpdate> _deviceStateSub;
  StreamSubscription<SigningState>? _signingStateSub;
  final HashSet<DeviceId> _connectedDevices = deviceIdSet([]);
  UnsignedTx? _unsignedTx;
  SignedTx? _signedTx;

  SigningState? _state;

  SigningSessionController() {
    _deviceStateSub = GlobalStreams.deviceListSubject.listen((update) {
      _connectedDevices.clear();
      _connectedDevices.addAll(update.state.devices.map((device) => device.id));
      if (hasListeners) notifyListeners();
      maybeRequestDeviceSign();
    });
  }

  @override
  void dispose() {
    _deviceStateSub.cancel();
    super.dispose();
  }

  Future<bool> init(UnsignedTx unsignedTx, Stream<SigningState> stream) async {
    if (_unsignedTx != null || _signingStateSub != null) return false;
    _unsignedTx = unsignedTx;
    _signingStateSub = stream.listen((state) async {
      final signatures = state.finishedSignatures;
      if (signatures != null && _unsignedTx != null) {
        _signedTx = _unsignedTx!.complete(signatures: signatures);
      }
      _state = state;
      if (hasListeners) notifyListeners();
      maybeRequestDeviceSign();
    });
    return true;
  }

  void cancel() async {
    if (_signingStateSub != null || _unsignedTx != null) {
      await coord.cancelProtocol();
      if (_state != null) {
        await coord.cancelSignSession(ssid: _state!.sessionId);
      }
      _signingStateSub?.cancel();
      _signingStateSub = null;
      _unsignedTx = null;
    }
  }

  SigningState? get state => _state;
  SignedTx? get signedTx => _signedTx;

  Iterable<DeviceSignatureModel>? mapDevices(Iterable<DeviceModel> devices) {
    if (_state == null) return null;
    return devices.map(
      (device) => DeviceSignatureModel(
        id: device.id,
        hasSignature: _state!.gotShares.any(
          (thisId) => deviceIdEquals(thisId, device.id),
        ),
        isConnected: _connectedDevices.contains(device.id),
      ),
    );
  }

  void maybeRequestDeviceSign() async {
    if (_state != null) {
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      for (final neededFrom in _state!.connectedButNeedRequest) {
        if (_connectedDevices.contains(neededFrom)) {
          coord.requestDeviceSign(
            deviceId: neededFrom,
            sessionId: _state!.sessionId,
            encryptionKey: encryptionKey,
          );
        }
      }
    }
  }
}
