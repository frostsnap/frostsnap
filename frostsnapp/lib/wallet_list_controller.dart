import 'dart:async';
import 'dart:core';

import 'package:flutter/material.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/src/rust/api.dart';
import 'package:frostsnapp/src/rust/api/bitcoin.dart';
import 'package:frostsnapp/src/rust/api/coordinator.dart';
import 'package:frostsnapp/src/rust/api/recovery.dart';

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

class WalletListController extends ChangeNotifier {
  late final StreamSubscription<KeyState> _sub;

  bool _gotInitialData = false;
  List<WalletItem> _wallets = [];
  int? _selectedIndex;

  WalletListController({required Stream<KeyState> keyStream}) {
    _sub = keyStream.listen((state) {
      _gotInitialData = true;
      _wallets = state.keys
          .map<WalletItem>((key) => WalletItemKey(key))
          .followedBy(
            state.restoring.map(
              (restoring) => WalletItemRestoration(restoring),
            ),
          )
          .toList();
      if (_selectedIndex == null || _selectedIndex! >= wallets.length) {
        _selectedIndex = _wallets.isNotEmpty ? 0 : null;
      }

      if (hasListeners) notifyListeners();
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    super.dispose();
  }

  bool get gotInitialData => _gotInitialData;
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

  selectWallet(KeyId? id) {
    if (id == null) {
      selectedIndex = null;
      return;
    }
    final walletIndex = wallets.indexWhere(
      (w) => switch (w) {
        WalletItemKey item => keyIdEquals(item.frostKey.keyId(), id),
        _ => false,
      },
    );
    if (walletIndex == selectedIndex) return;
    selectedIndex = walletIndex;
  }

  selectRecoveringWallet(RestorationId id) {
    final walletIndex = wallets.indexWhere(
      (w) => switch (w) {
        WalletItemRestoration item => restorationIdEquals(
          item.restoringKey.restorationId,
          id,
        ),
        _ => false,
      },
    );
    if (walletIndex == selectedIndex) return;
    selectedIndex = walletIndex;
  }
}

sealed class WalletItem {
  BitcoinNetwork? get network;
  String get name;
  Widget? get icon => null;
}

class WalletItemKey extends WalletItem {
  final FrostKey frostKey;

  WalletItemKey(this.frostKey);

  @override
  BitcoinNetwork? get network => frostKey.bitcoinNetwork();
  @override
  String get name => frostKey.keyName();

  Widget tryWrapInWalletContext({
    Key? key,
    required BuildContext context,
    required Widget child,
  }) {
    final superCtx = SuperWalletContext.of(context)!;
    return superCtx.tryWrapInWalletContext(
      key: key,
      keyId: frostKey.keyId(),
      child: child,
    );
  }
}

class WalletItemRestoration extends WalletItem {
  final RestoringKey restoringKey;
  WalletItemRestoration(this.restoringKey);

  @override
  BitcoinNetwork? get network => restoringKey.bitcoinNetwork;
  @override
  String get name => restoringKey.name;
  @override
  Widget? get icon => Icon(Icons.settings_backup_restore);
}
