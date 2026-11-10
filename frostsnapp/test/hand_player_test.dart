import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';

const _frame = Duration(milliseconds: 33);

/// A clip whose frames arrive only when the test releases them.
class _Clip implements HandClipFrames {
  _Clip(this.frameCount, this.images);

  @override
  final int frameCount;
  final List<ui.Image> images;
  final pending = <Completer<HandClipFrame>>[];
  var served = 0;
  var disposed = false;

  @override
  Future<HandClipFrame> next() {
    final completer = Completer<HandClipFrame>();
    pending.add(completer);
    return completer.future;
  }

  void release() {
    final image = images[served++];
    pending.removeAt(0).complete((image: image, duration: _frame));
  }

  void fail() => pending.removeAt(0).completeError(StateError('decode'));

  @override
  void dispose() => disposed = true;
}

HandPose pose(HandClipPlay play, int frames) =>
    HandPose(play, _frame * frames + const Duration(milliseconds: 16));

void main() {
  late List<_Clip> opened;
  late HandPlayer player;

  Future<_Clip> clip(int frames) async => _Clip(frames, [
    for (var i = 0; i < frames; i++) await createTestImage(cache: false),
  ]);

  Future<void> settle() => Future<void>.delayed(Duration.zero);

  Future<void> start(int frames) async {
    final next = await clip(frames);
    opened = [];
    player = HandPlayer(
      open: (asset) async {
        opened.add(next);
        return next;
      },
    );
  }

  const swipe = (asset: 'swipe', start: Duration.zero);

  test(
    'the next frame is decoded ahead and shown on the tick it falls due',
    () async {
      await start(5);
      player.show(pose(swipe, 0));
      await settle();
      final clip = opened.single;
      expect(clip.pending, hasLength(1));
      clip.release();
      await settle();
      expect(player.image, same(clip.images[0]));

      expect(clip.pending, hasLength(1), reason: 'frame 1 decodes ahead');
      clip.release();
      await settle();
      expect(player.image, same(clip.images[0]), reason: 'frame 1 is not due');
      expect(clip.pending, isEmpty, reason: 'one frame ahead, no more');

      player.show(pose(swipe, 1));
      expect(player.image, same(clip.images[1]));
      expect(clip.images[0].debugDisposed, isTrue);
      player.dispose();
    },
  );

  test('a late decode skips to the clock without queueing', () async {
    await start(10);
    var shown = 0;
    player.addListener(() => shown++);
    player.show(pose(swipe, 6));
    await settle();
    final clip = opened.single;
    for (var i = 0; i <= 6; i++) {
      expect(clip.pending, hasLength(1), reason: 'only one decode at a time');
      clip.release();
      await settle();
    }
    expect(player.image, same(clip.images[6]));
    expect(shown, 1, reason: 'frames whose time has passed are never shown');
    for (var i = 0; i < 6; i++) {
      expect(clip.images[i].debugDisposed, isTrue, reason: 'frame $i');
    }
    expect(clip.pending, hasLength(1), reason: 'only the next frame, ahead');
    player.dispose();
  });

  test(
    'past its end a clip rests on its last frame and stops decoding',
    () async {
      await start(3);
      player.show(pose(swipe, 100));
      await settle();
      final clip = opened.single;
      for (var i = 0; i < 3; i++) {
        clip.release();
        await settle();
      }
      expect(player.image, same(clip.images[2]));
      expect(clip.pending, isEmpty);
      player.dispose();
    },
  );

  test('a frame that arrives after the player is gone is let go', () async {
    await start(3);
    player.show(pose(swipe, 0));
    await settle();
    final clip = opened.single;
    player.dispose();
    expect(clip.disposed, isTrue);
    clip.release();
    await settle();
    expect(clip.images[0].debugDisposed, isTrue);
  });

  test('a new play of a clip opens it again from the start', () async {
    final clips = [await clip(2), await clip(2)];
    opened = [];
    player = HandPlayer(
      open: (asset) async {
        final next = clips[opened.length];
        opened.add(next);
        return next;
      },
    );
    player.show(pose(swipe, 1));
    await settle();
    for (var i = 0; i < 2; i++) {
      opened[0].release();
      await settle();
    }

    const again = (asset: 'swipe', start: Duration(seconds: 5));
    player.show(pose(again, 0));
    await settle();
    expect(opened, hasLength(2));
    expect(opened[0].disposed, isTrue);
    opened[1].release();
    await settle();
    expect(player.image, same(opened[1].images[0]));
    player.dispose();
  });

  group('a failure', () {
    late List<FlutterErrorDetails> errors;
    late FlutterExceptionHandler? handler;

    setUp(() {
      errors = [];
      handler = FlutterError.onError;
      FlutterError.onError = errors.add;
    });
    tearDown(() => FlutterError.onError = handler);

    test(
      'to open a clip is reported, not retried, and the next play still runs',
      () async {
        final good = await clip(1);
        var opens = 0;
        player = HandPlayer(
          open: (asset) async {
            opens++;
            if (asset == 'broken') throw StateError('open');
            return good;
          },
        );
        const broken = (asset: 'broken', start: Duration.zero);
        player.show(pose(broken, 0));
        await settle();
        player.show(pose(broken, 1));
        await settle();
        expect(errors, hasLength(1));
        expect(opens, 1);

        player.show(pose(swipe, 0));
        await settle();
        good.release();
        await settle();
        expect(player.image, same(good.images[0]));
        player.dispose();
      },
    );

    test(
      'to decode a frame is reported, not retried, and the next play still runs',
      () async {
        final clips = [await clip(3), await clip(1)];
        var opens = 0;
        player = HandPlayer(open: (asset) async => clips[opens++]);
        player.show(pose(swipe, 0));
        await settle();
        clips[0].fail();
        await settle();
        player.show(pose(swipe, 1));
        await settle();
        expect(errors, hasLength(1));
        expect(clips[0].disposed, isTrue);
        expect(clips[0].pending, isEmpty);

        const again = (asset: 'swipe', start: Duration(seconds: 5));
        player.show(pose(again, 0));
        await settle();
        clips[1].release();
        await settle();
        expect(player.image, same(clips[1].images[0]));
        player.dispose();
      },
    );
  });

  test(
    'a frame still decoding when a new play starts is never shown',
    () async {
      final clips = [await clip(2), await clip(2)];
      var opens = 0;
      player = HandPlayer(open: (asset) async => clips[opens++]);
      var shown = 0;
      player.addListener(() => shown++);
      player.show(pose(swipe, 0));
      await settle();

      const next = (asset: 'tap', start: Duration(seconds: 5));
      player.show(pose(next, 0));
      clips[0].release();
      await settle();
      expect(shown, 0);
      expect(clips[0].images[0].debugDisposed, isTrue);

      clips[1].release();
      await settle();
      expect(player.image, same(clips[1].images[0]));
      expect(shown, 1);
      player.dispose();
    },
  );
}
