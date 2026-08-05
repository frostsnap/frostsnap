import 'package:frostsnap/src/rust/api/bitcoin.dart';

extension BitcoinNetworkDisplayExt on BitcoinNetwork {
  /// User-facing network name. Unlike [name], this says which testnet version
  /// the wallet runs on ("testnet" is Testnet3, not Testnet4). [name] is the
  /// identity string that round-trips through [BitcoinNetwork.fromString], so
  /// it can't be changed for display purposes.
  String get displayName => switch (name()) {
    "bitcoin" => "Bitcoin",
    "testnet" => "Testnet3",
    "testnet4" => "Testnet4",
    "signet" => "Signet",
    "regtest" => "Regtest",
    final other => other,
  };
}
