import 'dart:collection';

import 'package:flutter/material.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/wallet.dart';

class FrostsnapContext extends InheritedWidget {
  final Stream<String> logStream;

  const FrostsnapContext({
    Key? key,
    required this.logStream,
    required Widget child,
  }) : super(key: key, child: child);

  // Static method to allow easy access to the Foo instance
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
  final Settings settings;

  SuperWalletContext({super.key, required super.child, required this.settings});

  final Map<KeyId, Stream<TxState>> txStreams = HashMap<KeyId, Stream<TxState>>(
    equals: (KeyId a, KeyId b) => keyIdEquals(a, b),
    hashCode: (KeyId key) => key.field0.hashCode,
  );

  // Static method to allow easy access to the Foo instance
  static SuperWalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<SuperWalletContext>();
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
    final superWallet = settings.getSuperWallet(network: bitcoinNetwork);
    final masterAppkey = frostKey.masterAppkey();
    if (masterAppkey == null) {
      return null;
    }
    final wallet = Wallet(superWallet: superWallet, masterAppkey: masterAppkey);

    if (!txStreams.containsKey(keyId)) {
      final stream =
          superWallet
              .subTxState(masterAppkey: masterAppkey)
              .toBehaviorSubject();
      txStreams[keyId] = stream;
    }

    return (wallet, txStreams[keyId]!);
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

    return WalletContext(
      key: key,
      wallet: wallet,
      txStream: txStream,
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

  WalletContext({
    super.key,
    required this.wallet,
    required this.txStream,
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
    return WalletContext(wallet: wallet, txStream: txStream, child: child);
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
