import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/secure_key_provider.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const channel = MethodChannel('com.frostsnap/secure_key');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() => messenger.setMockMethodCallHandler(channel, null));

  group('AndroidSecureKeyProvider.getOrCreateKey', () {
    test('returns the 32 key bytes the channel provides', () async {
      final bytes = Uint8List.fromList(List.generate(32, (i) => i + 1));
      messenger.setMockMethodCallHandler(channel, (call) async {
        expect(call.method, 'getOrCreateKey');
        return bytes;
      });

      final key = await AndroidSecureKeyProvider.forTesting().getOrCreateKey();

      expect(key.field0, orderedEquals(bytes));
    });

    test('falls back to the empty key on KEY_CREATION_FAILED', () async {
      messenger.setMockMethodCallHandler(channel, (call) async {
        throw PlatformException(code: 'KEY_CREATION_FAILED');
      });

      final key = await AndroidSecureKeyProvider.forTesting().getOrCreateKey();

      expect(key.field0.length, 32);
      expect(key.field0.every((b) => b == 0), isTrue);
      expect(key.field0, orderedEquals(SecureKeyProvider.emptyKey.field0));
    });

    test('rethrows a PlatformException with a different code', () async {
      messenger.setMockMethodCallHandler(channel, (call) async {
        throw PlatformException(code: 'NO_LOCK_SCREEN');
      });

      await expectLater(
        AndroidSecureKeyProvider.forTesting().getOrCreateKey(),
        throwsA(
          isA<PlatformException>().having((e) => e.code, 'code', 'NO_LOCK_SCREEN'),
        ),
      );
    });
  });
}
