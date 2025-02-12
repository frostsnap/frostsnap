import 'package:flutter/widgets.dart';
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
  int get accessStructureCount => key.accessStructures().length;

  bool get isRecovering => key
      .accessStructureState()
      .field0
      .every((accessStruct) => switch (accessStruct) {
            AccessStructureState_Recovering() => true,
            AccessStructureState_Complete() => false,
          });

  int? threshold(int accessIndex) {
    try {
      return key.accessStructures()[accessIndex].threshold();
    } catch (_) {
      return null;
    }
  }

  int? devices(int accessIndex) {
    return null;
  }
}

class WalletListController extends ChangeNotifier {}
