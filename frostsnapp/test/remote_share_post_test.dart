import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/remote_recovery_lobby_page.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_keygen.dart'
    show DeviceKind;
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/lib.dart';

// Unit tests for `sharePostFromRemoteResult` — the RemoteShareResult
// → SharePost conversion the lobby runs after the reused recovery
// flow pops. The opaque FRB types (PhysicalBackupPhase, RecoverShare,
// HeldShare2, ShareImage) are abstract Dart classes, so plain fakes
// stand in — the conversion only moves values, never calls the FFI.

DeviceId _did(int seed) {
  final bytes = Uint8List(33);
  for (var i = 0; i < 33; i++) {
    bytes[i] = seed;
  }
  return DeviceId(field0: U8Array33(bytes));
}

class _FakeShareImage implements ShareImage {
  @override
  void dispose() {}

  @override
  bool get isDisposed => false;
}

class _FakePhysicalBackupPhase implements PhysicalBackupPhase {
  _FakePhysicalBackupPhase({required DeviceId deviceId, required this.image})
    : _deviceId = deviceId;
  final DeviceId _deviceId;
  final ShareImage image;

  @override
  DeviceId deviceId() => _deviceId;

  @override
  ShareImage shareImage() => image;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;
}

class _FakeHeldShare2 implements HeldShare2 {
  _FakeHeldShare2({required this.image, required bool needsConsolidation})
    : _needsConsolidation = needsConsolidation;
  final ShareImage image;
  bool _needsConsolidation;

  @override
  ShareImage get shareImage => image;

  @override
  bool get needsConsolidation => _needsConsolidation;

  @override
  set needsConsolidation(bool v) => _needsConsolidation = v;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeRecoverShare implements RecoverShare {
  _FakeRecoverShare({required DeviceId heldBy, required HeldShare2 heldShare})
    : _heldBy = heldBy,
      _heldShare = heldShare;
  DeviceId _heldBy;
  HeldShare2 _heldShare;

  @override
  DeviceId get heldBy => _heldBy;

  @override
  set heldBy(DeviceId v) => _heldBy = v;

  @override
  HeldShare2 get heldShare => _heldShare;

  @override
  set heldShare(HeldShare2 v) => _heldShare = v;

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;
}

void main() {
  test('physical-backup arm: entered name, always needsConsolidation', () {
    final image = _FakeShareImage();
    final result = RemoteShareResultPhysicalBackup(
      phase: _FakePhysicalBackupPhase(deviceId: _did(0x11), image: image),
      deviceName: 'Alice device',
    );

    final post = sharePostFromRemoteResult(
      result,
      deviceNameOf: (_) => fail('must not consult coord for a typed name'),
    );

    expect(post.deviceId, _did(0x11));
    expect(post.deviceName, 'Alice device');
    expect(post.deviceKind, DeviceKind.frostsnap);
    expect(identical(post.shareImage, image), isTrue);
    expect(
      post.needsConsolidation,
      isTrue,
      reason:
          'An entered backup lives in device RAM until post-finalize '
          'consolidation — posting it as consolidated would skip the '
          'exit-recovery-mode round.',
    );
  });

  test('physical-backup arm: empty name falls back to resolver', () {
    final result = RemoteShareResultPhysicalBackup(
      phase: _FakePhysicalBackupPhase(
        deviceId: _did(0x22),
        image: _FakeShareImage(),
      ),
      deviceName: '',
    );

    final post = sharePostFromRemoteResult(
      result,
      deviceNameOf: (id) => id == _did(0x22) ? 'resolved' : null,
    );
    expect(post.deviceName, 'resolved');
  });

  for (final needsConsolidation in [true, false]) {
    test(
      'device-share arm: needsConsolidation=$needsConsolidation carried faithfully',
      () {
        final image = _FakeShareImage();
        final result = RemoteShareResultDeviceShare(
          share: _FakeRecoverShare(
            heldBy: _did(0x33),
            heldShare: _FakeHeldShare2(
              image: image,
              needsConsolidation: needsConsolidation,
            ),
          ),
        );

        final post = sharePostFromRemoteResult(
          result,
          deviceNameOf: (id) => 'Bob device',
        );

        expect(post.deviceId, _did(0x33));
        expect(post.deviceName, 'Bob device');
        expect(identical(post.shareImage, image), isTrue);
        expect(
          post.needsConsolidation,
          needsConsolidation,
          reason:
              'A device that already holds a proper share must not be '
              're-flagged for consolidation (and vice versa) — the flag '
              'comes from the share itself.',
        );
      },
    );
  }

  test('device-share arm: unknown device name falls back', () {
    final result = RemoteShareResultDeviceShare(
      share: _FakeRecoverShare(
        heldBy: _did(0x44),
        heldShare: _FakeHeldShare2(
          image: _FakeShareImage(),
          needsConsolidation: false,
        ),
      ),
    );

    final post = sharePostFromRemoteResult(result, deviceNameOf: (_) => null);
    expect(post.deviceName, 'device');
  });
}
