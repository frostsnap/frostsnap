import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';

SymmetricKey _key(int fill) =>
    SymmetricKey(field0: U8Array32(Uint8List(32)..fillRange(0, 32, fill)));

AccessStructureRef _asRef() => AccessStructureRef(
  keyId: KeyId(field0: U8Array32(Uint8List(32))),
  accessStructureId: AccessStructureId(field0: U8Array32(Uint8List(32))),
);

void main() {
  group('existingWalletKey routing', () {
    test('key unavailable -> shows recovery and returns null', () async {
      var recoveryShown = 0;
      final result = await existingWalletKey(
        accessStructureRef: _asRef(),
        action: 'sign this message',
        getKey: () async => throw const WalletKeyUnavailable(),
        canDecrypt: (_) =>
            fail('canDecrypt must not run when the key is unavailable'),
        showRecovery: () async => recoveryShown++,
      );

      expect(result, isNull);
      expect(recoveryShown, 1);
    });

    test('wrong key (cannot decrypt) -> shows recovery and returns null', () async {
      final wrongKey = _key(9);
      var recoveryShown = 0;
      final result = await existingWalletKey(
        accessStructureRef: _asRef(),
        action: 'sign this message',
        getKey: () async => wrongKey,
        canDecrypt: (key) {
          expect(key, same(wrongKey));
          return false;
        },
        showRecovery: () async => recoveryShown++,
      );

      expect(result, isNull);
      expect(recoveryShown, 1);
    });

    test('correct key (can decrypt) -> returns the key, no recovery', () async {
      final goodKey = _key(7);
      var recoveryShown = 0;
      final result = await existingWalletKey(
        accessStructureRef: _asRef(),
        action: 'sign this message',
        getKey: () async => goodKey,
        canDecrypt: (_) => true,
        showRecovery: () async => recoveryShown++,
      );

      expect(result, same(goodKey));
      expect(recoveryShown, 0);
    });
  });
}
