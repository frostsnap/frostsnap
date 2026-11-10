import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

import 'support/rust_lib.dart';

const _clips = [
  'assets/devices/hand/swipe.webp',
  'assets/devices/hand/hold-press.webp',
  'assets/devices/hand/hold-release.webp',
  'assets/devices/hand/tap.webp',
];

int _bytes(ui.Image image) => image.width * image.height * 4;

/// Every frame handed to a player, and the most decoded hand pixels alive at once. It samples as
/// each frame arrives, before the player lets the one it replaces go.
class _Meter {
  final seen = <ui.Image>[];
  var peak = 0;

  OpenHandClip open() =>
      (asset) async => _Metered(await HandPlayer.openAsset(asset), this);

  void arrived(ui.Image image) {
    seen.add(image);
    final alive = seen
        .where((image) => !image.debugDisposed)
        .fold(0, (sum, image) => sum + _bytes(image));
    if (alive > peak) peak = alive;
  }

  Iterable<ui.Image> get leaked => seen.where((image) => !image.debugDisposed);
}

class _Metered implements HandClipFrames {
  _Metered(this._inner, this._meter);

  final HandClipFrames _inner;
  final _Meter _meter;

  @override
  int get frameCount => _inner.frameCount;

  @override
  Future<HandClipFrame> next() async {
    final frame = await _inner.next();
    _meter.arrived(frame.image);
    return frame;
  }

  @override
  void dispose() => _inner.dispose();
}

String _kb(int bytes) => '${(bytes / 1e3).toStringAsFixed(0)} KB';

void main() {
  setUpAll(initRustLibForTests);

  testWidgets('every clip plays through with a frame or two in memory', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final meter = _Meter();
      final player = HandPlayer(open: meter.open());
      var start = Duration.zero;
      for (final asset in _clips) {
        start += const Duration(minutes: 1);
        final play = (asset: asset, start: start);
        for (var ms = 0; ms <= 8000; ms += 33) {
          player.show(HandPose(play, Duration(milliseconds: ms)));
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
      }
      player.dispose();
      expect(meter.leaked, isEmpty);

      var sheets = 0;
      for (final name in ['hand-swipe-sheet.webp', 'hand-hold-sheet.webp']) {
        final codec = await ui.instantiateImageCodec(
          File('../tools/hand_gestures/source/$name').readAsBytesSync(),
        );
        final sheet = (await codec.getNextFrame()).image;
        sheets += _bytes(sheet);
        sheet.dispose();
        codec.dispose();
      }
      final files = [for (final asset in _clips) File(asset).lengthSync()];
      debugPrint(
        'hand memory, all clips: decoded frames peak ${_kb(meter.peak)} over '
        '${meter.seen.length} frames; the old sheets decode to ${_kb(sheets)}; '
        'clip files ${files.map(_kb).join(', ')}',
      );
      expect(meter.peak, lessThan(2 * 1000 * 1000));
    });
  });

  testWidgets('the sign and erase heroes keep the hand under 2 MB', (
    tester,
  ) async {
    await tester.runAsync(() async {
      const seconds = 18;
      final heroes = [
        for (final (name, kind) in [
          ('sign', const DeviceDemoKind.sign()),
          ('erase', const DeviceDemoKind.erase()),
        ])
          () {
            final meter = _Meter();
            return (
              name: name,
              meter: meter,
              playback: DeviceDemoPlayback(kind, openHandClip: meter.open()),
            );
          }(),
      ];
      final ticks = Timer.periodic(const Duration(milliseconds: 16), (_) {
        for (final hero in heroes) {
          hero.playback.tickHand();
        }
      });
      await Future<void>.delayed(const Duration(seconds: seconds));
      ticks.cancel();
      for (final hero in heroes) {
        hero.playback.dispose();
        expect(hero.meter.leaked, isEmpty, reason: hero.name);
        expect(hero.meter.seen, isNotEmpty, reason: hero.name);
        expect(hero.meter.peak, lessThan(2 * 1000 * 1000), reason: hero.name);
        debugPrint(
          'hand memory, ${hero.name} hero over $seconds s: decoded frames peak '
          '${_kb(hero.meter.peak)} over ${hero.meter.seen.length} frames',
        );
      }
    });
  });
}
