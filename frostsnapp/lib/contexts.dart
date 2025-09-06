import 'package:confetti/confetti.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_manager.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/init.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/psbt_manager.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_list_controller.dart';

class FrostsnapContext extends InheritedWidget {
  final Stream<String> logStream;
  final AppCtx appCtx;

  const FrostsnapContext({
    super.key,
    required this.logStream,
    required this.appCtx,
    required super.child,
  });

  static FrostsnapContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<FrostsnapContext>();
  }

  @override
  bool updateShouldNotify(FrostsnapContext oldWidget) {
    // we never change the log stream
    return false;
  }

  BackupManager get backupManager => appCtx.backupManager;
  PsbtManager get psbtManager => appCtx.psbtManager;
}

class SuperWalletContext extends InheritedWidget {
  final AppCtx appCtx;

  SuperWalletContext({super.key, required super.child, required this.appCtx});

  final Map<KeyId, Stream<TxState>> _txStreams = keyIdMap();
  final Map<KeyId, Stream<void>> _signingSessionSignals = keyIdMap();
  final Map<KeyId, Stream<BackupRun>> _backupStreams = keyIdMap();

  // Static method to allow easy access to the Foo instance
  static SuperWalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<SuperWalletContext>();
  }

  Stream<BackupRun> backupStream(KeyId keyId) {
    var stream = _backupStreams[keyId];
    if (stream == null) {
      stream = appCtx.backupManager
          .backupStream(keyId: keyId)
          .toBehaviorSubject();
      _backupStreams[keyId] = stream;
    }
    return stream;
  }

  Stream<void> signingSessionSignalStream(KeyId keyId) {
    var stream = _signingSessionSignals[keyId];
    if (stream == null) {
      stream = coord.subSigningSessionSignals(keyId: keyId).toBehaviorSubject();
      _signingSessionSignals[keyId] = stream;
    }
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
    var stream = _txStreams[keyId];
    if (stream == null) {
      stream = superWallet
          .subTxState(masterAppkey: masterAppkey)
          .toBehaviorSubject();
      _txStreams[keyId] = stream;
    }

    return (wallet, stream);
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
         child: KeyContext(keyId: wallet.masterAppkey.keyId(), child: child),
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

  SuperWallet get superWallet => wallet.superWallet;
  MasterAppkey get masterAppkey => wallet.masterAppkey;
  KeyId get keyId => wallet.masterAppkey.keyId();
  BitcoinNetwork get network => wallet.superWallet.network;
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

  String get name => frostKey().keyName();

  @override
  bool updateShouldNotify(KeyContext oldWidget) {
    return false;
  }
}

class HomeContext extends InheritedWidget {
  final GlobalKey<ScaffoldState> scaffoldKey;
  final WalletListController walletListController;
  final ConfettiController confettiController;

  const HomeContext({
    super.key,
    required this.scaffoldKey,
    required this.walletListController,
    required this.confettiController,
    required super.child,
  });

  static HomeContext? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HomeContext>();

  HomeContext wrap(Widget child) => HomeContext(
    scaffoldKey: scaffoldKey,
    walletListController: walletListController,
    confettiController: confettiController,
    child: child,
  );

  void openNewlyCreatedWallet(KeyId id) {
    walletListController.selectWallet(id);
    scaffoldKey.currentState?.closeDrawer();
    confettiController.play();
  }

  @override
  bool updateShouldNotify(covariant InheritedWidget oldWidget) => false;
}
