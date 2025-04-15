import 'dart:collection';

import 'package:confetti/confetti.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/wallet_list_controller.dart';

class FrostsnapContext extends InheritedWidget {
  final Stream<String> logStream;
  final BackupManager backupManager;
  final AppCtx appCtx;

  const FrostsnapContext({
    Key? key,
    required this.logStream,
    required this.backupManager,
    required this.appCtx,
    required Widget child,
  }) : super(key: key, child: child);

  static FrostsnapContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<FrostsnapContext>();
  }

  @override
  bool updateShouldNotify(FrostsnapContext oldWidget) {
    // we never change the log stream
    return false;
  }
}

class SuperWalletContext extends InheritedWidget {
  final AppCtx appCtx;

  SuperWalletContext({super.key, required super.child, required this.appCtx});

  final Map<KeyId, Stream<TxState>> _txStreams = HashMap(
    equals: keyIdEquals,
    hashCode: (key) => key.field0.hashCode,
  );

  final Map<KeyId, Stream<void>> _signingSessionSignals = HashMap(
    equals: keyIdEquals,
    hashCode: (key) => key.field0.hashCode,
  );

  final Map<KeyId, Stream<BackupRun>> _backupStreams = HashMap(
    equals: keyIdEquals,
    hashCode: (key) => key.field0.hashCode,
  );

  // Static method to allow easy access to the Foo instance
  static SuperWalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<SuperWalletContext>();
  }

  Stream<BackupRun> backupStream(KeyId keyId) {
    final stream = _backupStreams[keyId];
    if (stream != null) return stream;
    _backupStreams[keyId] =
        appCtx.backupManager.backupStream(keyId: keyId).toBehaviorSubject();
    return _backupStreams[keyId]!;
  }

  Stream<void> signingSessionSignalStream(KeyId keyId) {
    var stream = _signingSessionSignals[keyId];
    if (stream != null) return stream;
    stream = coord.subSigningSessionSignals(keyId: keyId);
    _signingSessionSignals[keyId] = stream;
    return stream;
  }

  (Wallet, Stream<TxState>)? txStateStream(KeyId keyId) {
    final frostKey = coord.getFrostKey(keyId: keyId);
    if (frostKey == null) {
      return null;
    }
    final bitcoinNetwork = frostKey.bitcoinNetwork();
    if (bitcoinNetwork == null) {
      return null;
    }

    final superWallet = appCtx.settings.getSuperWallet(network: bitcoinNetwork);
    final masterAppkey = frostKey.masterAppkey();
    final wallet = Wallet(superWallet: superWallet, masterAppkey: masterAppkey);

    // Get or create tx stream
    if (!_txStreams.containsKey(keyId)) {
      final stream =
          superWallet
              .subTxState(masterAppkey: masterAppkey)
              .toBehaviorSubject();
      _txStreams[keyId] = stream;
    }

    return (wallet, _txStreams[keyId]!);
  }

  Widget tryWrapInWalletContext({
    required KeyId keyId,
    required Widget child,
    Key? key,
  }) {
    final record = txStateStream(keyId);

    if (record == null) {
      // This key doesn't have a full wallet
      return KeyContext(keyId: keyId, child: child);
    }
    final wallet = record.$1;
    final txStream = record.$2;
    final frostKey = wallet.frostKey();

    if (frostKey != null) {
      appCtx.backupManager.maybeStartBackupRun(
        accessStructure:
            frostKey
                .accessStructures()
                .first, // assuming one access structure for now.
      );
    }

    final backupStream = this.backupStream(keyId);
    final signingSessionSignals = signingSessionSignalStream(keyId);

    return WalletContext(
      key: key,
      wallet: wallet,
      txStream: txStream,
      backupStream: backupStream,
      signingSessionSignals: signingSessionSignals,
      child: child,
    );
  }

  @override
  bool updateShouldNotify(InheritedWidget oldWidget) {
    // WalletCtx is never changed
    return false;
  }
}

class WalletContext extends InheritedWidget {
  final Wallet wallet;
  final Stream<TxState> txStream;
  final Stream<BackupRun> backupStream;
  final Stream<void> signingSessionSignals;

  WalletContext({
    super.key,
    required this.wallet,
    required this.txStream,
    required this.backupStream,
    required this.signingSessionSignals,
    required Widget child,
  }) : super(
         // a wallet context implies a key context so we wrap the child in one also
         child: KeyContext(
           keyId: api.masterAppkeyExtToKeyId(masterAppkey: wallet.masterAppkey),
           child: child,
         ),
       );

  static WalletContext? of(BuildContext context) {
    return context.getInheritedWidgetOfExactType<WalletContext>();
  }

  /// so we can clone this context over a new widget tree
  WalletContext wrap(Widget child) {
    return WalletContext(
      wallet: wallet,
      txStream: txStream,
      backupStream: backupStream,
      signingSessionSignals: signingSessionSignals,
      child: child,
    );
  }

  @override
  bool updateShouldNotify(WalletContext oldWidget) {
    // never updates
    return false;
  }

  get superWallet => wallet.superWallet;
  get masterAppkey => wallet.masterAppkey;
  get keyId => api.masterAppkeyExtToKeyId(masterAppkey: wallet.masterAppkey);
  get network => wallet.superWallet.network;
}

class KeyContext extends InheritedWidget {
  final KeyId keyId;

  const KeyContext({super.key, required super.child, required this.keyId});

  static KeyContext? of(BuildContext context) {
    return context.getInheritedWidgetOfExactType<KeyContext>();
  }

  KeyContext wrap(Widget child) {
    return KeyContext(keyId: keyId, child: child);
  }

  FrostKey frostKey() {
    return coord.getFrostKey(keyId: keyId)!;
  }

  @override
  bool updateShouldNotify(KeyContext oldWidget) {
    return false;
  }
}

class HomeContext extends InheritedWidget {
  final GlobalKey<ScaffoldState> scaffoldKey;
  final WalletListController walletListController;
  final ConfettiController confettiController;
  final ValueNotifier<bool> isShowingCreatedWalletDialog;

  const HomeContext({
    super.key,
    required this.scaffoldKey,
    required this.walletListController,
    required this.confettiController,
    required this.isShowingCreatedWalletDialog,
    required Widget child,
  }) : super(child: child);

  static HomeContext? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HomeContext>();

  HomeContext wrap(Widget child) => HomeContext(
    scaffoldKey: scaffoldKey,
    walletListController: walletListController,
    confettiController: confettiController,
    isShowingCreatedWalletDialog: isShowingCreatedWalletDialog,
    child: child,
  );

  WalletItem? openNewlyCreatedWallet(KeyId id) {
    walletListController.selectedId = id;
    scaffoldKey.currentState?.closeDrawer();
    confettiController.play();
    return walletListController.selected;
  }

  @override
  bool updateShouldNotify(covariant InheritedWidget oldWidget) => false;
}
