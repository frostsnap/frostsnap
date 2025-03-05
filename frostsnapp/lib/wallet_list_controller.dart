import 'dart:async';
import 'dart:core';

import 'package:collection/collection.dart';
import 'package:flutter/widgets.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/id_ext.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class AccessStructureSummaryItem {
  final FrostKey key;
  final int index;
  final int threshold;
  final int? devices;

  const AccessStructureSummaryItem({
    required this.key,
    required this.index,
    required this.threshold,
    this.devices,
  });
}

class WalletItem {
  final FrostKey key;

  const WalletItem(this.key);

  KeyId get id => key.keyId();
  String get name => key.keyName();
  BitcoinNetwork? get network => key.bitcoinNetwork();

  /// Number of access structures.
  int get accessCount => key.accessStructures().length;
  int get recoveringAccessCount => key.accessStructureState().field0.length;

  List<AccessStructureId> get accessIds =>
      key.accessStructures().map((a) => a.id()).toList();
  Iterable<AccessStructureId> get recoveringAccessIds => key
      .accessStructureState()
      .field0
      .map<AccessStructureId?>(
        (accessState) => switch (accessState) {
          AccessStructureState_Complete() => null,
          AccessStructureState_Recovering(:final field0) =>
            field0.accessStructureId,
        },
      )
      .where((id) => id != null)
      .map((id) => id!);

  bool get isRecovering => key.accessStructureState().field0.every(
    (accessStruct) => switch (accessStruct) {
      AccessStructureState_Recovering() => true,
      AccessStructureState_Complete() => false,
    },
  );

  int? thesholdFor(AccessStructureId accessId) {
    final state = key
        .accessStructureState()
        .field0
        .map(
          (state) => switch (state) {
            AccessStructureState_Complete(:final field0) => (
              field0.id(),
              field0.threshold(),
            ),
            AccessStructureState_Recovering(:final field0) => (
              field0.accessStructureId,
              field0.threshold,
            ),
          },
        )
        .firstWhereOrNull(
          (state) => state.$1.field0.toString() == accessId.field0.toString(),
        );
    return state?.$2;
  }

  List<DeviceId>? devicesFor(AccessStructureId accessId) {
    final state = key
        .accessStructureState()
        .field0
        .map(
          (state) => switch (state) {
            AccessStructureState_Complete(:final field0) => (
              field0.id(),
              field0.devices(),
            ),
            AccessStructureState_Recovering(:final field0) => (
              field0.accessStructureId,
              field0.gotSharesFrom,
            ),
          },
        )
        .firstWhereOrNull(
          (state) => state.$1.field0.toString() == accessId.field0.toString(),
        );
    return state?.$2;
  }

  Widget tryWrapInWalletContext({
    Key? key,
    required BuildContext context,
    required Widget child,
  }) {
    final superCtx = SuperWalletContext.of(context)!;
    return superCtx.tryWrapInWalletContext(
      key: key,
      keyId: this.key.keyId(),
      child: child,
    );
  }
}

class WalletListController extends ChangeNotifier {
  late final StreamSubscription<KeyState> _sub;

  bool _gotInitialData = false;
  List<WalletItem> _wallets = [];
  int? _selectedIndex;
  List<WalletItem> _recovering = [];
  List<RecoverableKey> _recoverables = [];

  WalletListController({required Stream<KeyState> keyStream}) {
    _sub = keyStream.listen((state) {
      _gotInitialData = true;
      _wallets =
          state.keys
              .map((key) => WalletItem(key))
              .where((key) => !key.isRecovering)
              .toList();
      _recovering =
          state.keys
              .map((key) => WalletItem(key))
              .where((key) => key.isRecovering)
              .toList();
      _selectedIndex = _wallets.isNotEmpty ? 0 : null;
      _recoverables = state.recoverable;
      if (hasListeners) notifyListeners();
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    super.dispose();
  }

  bool get gotInitalData => _gotInitialData;
  List<WalletItem> get wallets => _wallets;

  int? get selectedIndex =>
      (_selectedIndex != null &&
              _selectedIndex! < _wallets.length &&
              _selectedIndex! >= 0)
          ? _selectedIndex
          : null;
  set selectedIndex(int? index) {
    if (index == _selectedIndex) return;
    if (index != null && index >= _wallets.length) return;
    _selectedIndex = index;
    if (hasListeners) notifyListeners();
  }

  WalletItem? get selected {
    final index = selectedIndex;
    if (index == null) return null;
    return _wallets[index];
  }

  KeyId? get selectedId => selected?.id;
  set selectedId(KeyId? id) {
    if (id == null) {
      selectedIndex = null;
      return;
    }
    final walletIndex = wallets.indexWhere((w) => keyIdEquals(w.id, id));
    if (walletIndex == selectedIndex) return;
    selectedIndex = walletIndex;
  }

  List<WalletItem> get recovering => _recovering;
  List<RecoverableKey> get recoverables => _recoverables;
}
