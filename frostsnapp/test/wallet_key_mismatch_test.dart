import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';

SymmetricKey _key(int fill) =>
    SymmetricKey(field0: U8Array32(Uint8List(32)..fillRange(0, 32, fill)));

AccessStructureRef _asRef([int fill = 0]) => AccessStructureRef(
  keyId: KeyId(field0: U8Array32(Uint8List(32)..fillRange(0, 32, fill))),
  accessStructureId: AccessStructureId(
    field0: U8Array32(Uint8List(32)..fillRange(0, 32, fill)),
  ),
);

DeviceId _deviceId() => DeviceId(field0: U8Array33(Uint8List(33)));

void main() {
  group('existingWalletKey routing', () {
    test('key unavailable -> shows recovery and returns null', () async {
      var recoveryShown = 0;
      final result = await existingWalletKey(
        accessStructureRef: _asRef(),
        action: 'sign this message',
        getKey: () async => throw const WalletKeyUnavailable(),
        // Only the empty-key fallback probe may run when the key is unavailable.
        canDecrypt: (key) {
          expect(key.field0, orderedEquals(SecureKeyProvider.emptyKey.field0));
          return false;
        },
        showRecovery: () async => recoveryShown++,
      );

      expect(result, isNull);
      expect(recoveryShown, 1);
    });

    test(
      'wrong key (cannot decrypt) -> shows recovery and returns null',
      () async {
        final wrongKey = _key(9);
        final probedKeys = <SymmetricKey>[];
        var recoveryShown = 0;
        final result = await existingWalletKey(
          accessStructureRef: _asRef(),
          action: 'sign this message',
          getKey: () async => wrongKey,
          canDecrypt: (key) {
            probedKeys.add(key);
            return false;
          },
          showRecovery: () async => recoveryShown++,
        );

        expect(result, isNull);
        expect(recoveryShown, 1);
        // The fetched key first, then the empty-key fallback probe.
        expect(probedKeys.first, same(wrongKey));
        expect(
          probedKeys.last.field0,
          orderedEquals(SecureKeyProvider.emptyKey.field0),
        );
      },
    );

    test(
      'key unavailable but empty key decrypts -> returns empty key, no recovery',
      () async {
        var recoveryShown = 0;
        final result = await existingWalletKey(
          accessStructureRef: _asRef(),
          action: 'sign this message',
          getKey: () async => throw const WalletKeyUnavailable(),
          canDecrypt: (key) => key.field0.every((b) => b == 0),
          showRecovery: () async => recoveryShown++,
        );

        expect(result, isNotNull);
        expect(
          result!.field0,
          orderedEquals(SecureKeyProvider.emptyKey.field0),
        );
        expect(recoveryShown, 0);
      },
    );

    test(
      'wrong key but empty key decrypts -> returns empty key, no recovery',
      () async {
        var recoveryShown = 0;
        final result = await existingWalletKey(
          accessStructureRef: _asRef(),
          action: 'sign this message',
          getKey: () async => _key(9),
          canDecrypt: (key) => key.field0.every((b) => b == 0),
          showRecovery: () async => recoveryShown++,
        );

        expect(result, isNotNull);
        expect(
          result!.field0,
          orderedEquals(SecureKeyProvider.emptyKey.field0),
        );
        expect(recoveryShown, 0);
      },
    );

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

  group('exitRecoveryModeForDevice', () {
    test('empty set -> no key fetch, no exit', () async {
      var gotKey = 0;
      var exited = 0;
      await exitRecoveryModeForDevice(
        deviceId: _deviceId(),
        pendingConsolidations: const [],
        getKey: (_) async {
          gotKey++;
          return _key(1);
        },
        canDecrypt: (_, _) => true,
        exit: (_) => exited++,
        onMismatch: () async {},
      );
      expect(gotKey, 0);
      expect(exited, 0);
    });

    test('key decrypts every wallet -> exits with that key', () async {
      final key = _key(7);
      SymmetricKey? exitedWith;
      var mismatchShown = 0;
      final checked = <AccessStructureRef>[];
      await exitRecoveryModeForDevice(
        deviceId: _deviceId(),
        pendingConsolidations: [_asRef(1), _asRef(2)],
        getKey: (_) async => key,
        canDecrypt: (ref, _) {
          checked.add(ref);
          return true;
        },
        exit: (k) => exitedWith = k,
        onMismatch: () async => mismatchShown++,
      );
      expect(exitedWith, same(key));
      expect(mismatchShown, 0);
      // Every wallet was verified before exiting.
      expect(checked.length, 2);
    });

    test(
      'key decrypts the first wallet but not a later one -> no exit, shows mismatch',
      () async {
        final key = _key(7);
        final refA = _asRef(1);
        final refB = _asRef(2);
        var exited = 0;
        var mismatchShown = 0;
        await exitRecoveryModeForDevice(
          deviceId: _deviceId(),
          pendingConsolidations: [refA, refB],
          getKey: (_) async => key,
          // Decrypts wallet A but not wallet B: a genuine whole-set mismatch.
          canDecrypt: (ref, _) => ref == refA,
          exit: (_) => exited++,
          onMismatch: () async => mismatchShown++,
        );
        expect(exited, 0);
        expect(mismatchShown, 1);
      },
    );

    test('no candidate key (null) -> no exit, no mismatch dialog here', () async {
      var exited = 0;
      var mismatchShown = 0;
      await exitRecoveryModeForDevice(
        deviceId: _deviceId(),
        pendingConsolidations: [_asRef(1)],
        // existingWalletKey returning null means it already surfaced the problem.
        getKey: (_) async => null,
        canDecrypt: (_, _) => true,
        exit: (_) => exited++,
        onMismatch: () async => mismatchShown++,
      );
      expect(exited, 0);
      expect(mismatchShown, 0);
    });
  });
}
