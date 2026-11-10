import 'dart:async';
import 'dart:ui' as ui;

import 'package:collection/collection.dart';
import 'package:flutter/animation.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

/// One play of a hand clip: its asset, from when it started.
typedef HandClipPlay = ({String asset, Duration start});

/// A play of a hand clip, shown [elapsed] into it. Past its end, it shows its last frame.
@immutable
class HandPose {
  const HandPose(this.play, this.elapsed);

  final HandClipPlay play;
  final Duration elapsed;
}

/// Which hand clip to show, and how far into it, given the gestures the device demo cued. The
/// clips and their timing come with each cue.
class HandTrack {
  final _clips = <HandClipPlay>[];
  Duration _settle = Duration.zero;
  ({Duration at, Offset offset}) _from = (
    at: Duration.zero,
    offset: Offset.zero,
  );
  ({Duration at, Offset offset}) _to = (at: Duration.zero, offset: Offset.zero);

  void cue(DeviceDemoCue cue, Duration at) {
    // Until the new gesture's first clip starts, the hand stays on the clip it last played.
    final shown = _clips.lastWhereOrNull((clip) => clip.start <= at);
    _clips
      ..clear()
      ..addAll([if (shown != null) shown])
      ..addAll([
        for (final clip in cue.clips)
          (
            asset: clip.asset,
            start: at + Duration(milliseconds: clip.startInMs),
          ),
      ]);
    _settle = at + Duration(milliseconds: cue.settleInMs);
    _from = (at: at, offset: offsetAt(at));
    _to = (
      at: at + Duration(milliseconds: cue.contactInMs),
      offset: Offset(cue.offsetX.toDouble(), cue.offsetY.toDouble()),
    );
  }

  /// Null until the first clip starts: a demo that never cues a gesture has no hand.
  HandPose? poseAt(Duration now) {
    final clip = _clips.lastWhereOrNull((clip) => clip.start <= now);
    return clip == null ? null : HandPose(clip, now - clip.start);
  }

  /// How far from where the clips put the fingertip the hand is, in the device screen's pixels.
  /// It moves onto each gesture's press point as the finger reaches for it.
  Offset offsetAt(Duration now) {
    if (now >= _to.at) return _to.offset;
    if (now <= _from.at) return _from.offset;
    final t = Curves.easeInOut.transform(
      (now - _from.at).inMicroseconds / (_to.at - _from.at).inMicroseconds,
    );
    return Offset.lerp(_from.offset, _to.offset, t) ?? _to.offset;
  }

  /// Whether the hand is still going, rather than resting on its last frame.
  bool isMovingAt(Duration now) => now < _settle;
}

typedef HandClipFrame = ({ui.Image image, Duration duration});

/// A hand clip's frames, decoded one at a time in order.
abstract interface class HandClipFrames {
  int get frameCount;

  /// The next frame and how long it shows.
  Future<HandClipFrame> next();

  void dispose();
}

typedef OpenHandClip = Future<HandClipFrames> Function(String asset);

class _CodecFrames implements HandClipFrames {
  _CodecFrames(this._codec);

  final ui.Codec _codec;

  @override
  int get frameCount => _codec.frameCount;

  @override
  Future<HandClipFrame> next() async {
    final frame = await _codec.getNextFrame();
    return (image: frame.image, duration: frame.duration);
  }

  @override
  void dispose() => _codec.dispose();
}

/// Shows a [HandPose] from its clip, decoding the clip's frames in order so only the frame on
/// show and the one being decoded are ever in memory, not whole clips.
class HandPlayer extends ChangeNotifier {
  HandPlayer({OpenHandClip open = openAsset}) : _open = open;

  /// Decodes the app's bundled clip with Flutter's codec.
  static Future<HandClipFrames> openAsset(String asset) async {
    final data = await rootBundle.load(asset);
    return _CodecFrames(
      await ui.instantiateImageCodec(data.buffer.asUint8List()),
    );
  }

  final OpenHandClip _open;
  HandPose? _want;
  HandClipPlay? _playing;
  HandClipPlay? _failed;
  HandClipFrames? _frames;
  var _decoded = 0;
  var _decodedUntil = Duration.zero;
  ({ui.Image image, Duration from, Duration until})? _ahead;
  ui.Image? _image;
  var _busy = false;
  var _disposed = false;

  /// The frame on show, kept until the next one is ready so the hand never blinks out.
  ui.Image? get image => _image;

  void show(HandPose? pose) {
    if (pose == null) return;
    _want = pose;
    _pump();
  }

  // The next frame is decoded ahead and swapped in on the tick it falls due, so a frame never
  // waits on its own decode. One open or decode is in flight at a time. When decoding falls
  // behind the clock, frames whose time has already passed are dropped rather than shown late.
  // A play that failed isn't retried: the next cue's play starts afresh.
  void _pump() {
    final want = _want;
    if (_disposed || want == null || want.play == _failed) return;
    if (want.play != _playing) {
      if (!_busy) {
        _busy = true;
        unawaited(_openPlay(want.play));
      }
      return;
    }
    _promote(want);
    final frames = _frames;
    if (_busy ||
        frames == null ||
        _ahead != null ||
        _decoded >= frames.frameCount) {
      return;
    }
    _busy = true;
    unawaited(_decodeNext(frames));
  }

  void _promote(HandPose want) {
    final ahead = _ahead;
    if (ahead == null || want.elapsed < ahead.from) return;
    _ahead = null;
    final frames = _frames;
    final passed =
        want.elapsed >= ahead.until &&
        frames != null &&
        _decoded < frames.frameCount;
    if (passed) {
      ahead.image.dispose();
    } else {
      _image?.dispose();
      _image = ahead.image;
      notifyListeners();
    }
  }

  Future<void> _openPlay(HandClipPlay play) async {
    _frames?.dispose();
    _frames = null;
    _ahead?.image.dispose();
    _ahead = null;
    _playing = play;
    _decoded = 0;
    _decodedUntil = Duration.zero;
    try {
      final frames = await _open(play.asset);
      if (_disposed || _want?.play != play) {
        frames.dispose();
      } else {
        _frames = frames;
      }
    } catch (error, stack) {
      _fail(play, error, stack);
    } finally {
      _busy = false;
    }
    _pump();
  }

  Future<void> _decodeNext(HandClipFrames frames) async {
    final play = _playing;
    try {
      final frame = await frames.next();
      if (_disposed || !identical(frames, _frames) || _want?.play != play) {
        frame.image.dispose();
      } else {
        final from = _decodedUntil;
        _decoded++;
        _decodedUntil += frame.duration;
        _ahead = (image: frame.image, from: from, until: _decodedUntil);
      }
    } catch (error, stack) {
      if (identical(frames, _frames)) {
        frames.dispose();
        _frames = null;
      }
      _fail(play, error, stack);
    } finally {
      _busy = false;
    }
    _pump();
  }

  void _fail(HandClipPlay? play, Object error, StackTrace stack) {
    _failed = play;
    FlutterError.reportError(
      FlutterErrorDetails(
        exception: error,
        stack: stack,
        library: 'device demo',
        context: ErrorDescription('while playing a hand clip'),
      ),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _frames?.dispose();
    _frames = null;
    _ahead?.image.dispose();
    _ahead = null;
    _image?.dispose();
    _image = null;
    super.dispose();
  }
}

/// A running device demo: its latest screen and where the hand is.
class DeviceDemoPlayback extends ChangeNotifier {
  DeviceDemoPlayback(DeviceDemoKind kind, {OpenHandClip? openHandClip})
    : _demo = DeviceDemo(kind: kind),
      handPlayer = HandPlayer(open: openHandClip ?? HandPlayer.openAsset) {
    _clock.start();
    _events = _demo.events().listen(_onEvent);
  }

  final DeviceDemo _demo;
  late final StreamSubscription<DeviceDemoEvent> _events;
  final _clock = Stopwatch();
  final hand = HandTrack();
  final HandPlayer handPlayer;
  ui.Image? _first;
  ui.Image? _screen;
  bool _decoding = false;
  Uint8List? _pending;
  bool _disposed = false;

  ui.Image? get screen => _screen;

  /// The screen the demo opened on, which a device the hand never touches keeps showing.
  ui.Image? get firstScreen => _first;

  Duration get now => _clock.elapsed;

  /// Moves the hand's clip on to now. Call it each frame while the hand moves.
  void tickHand() => handPlayer.show(hand.poseAt(now));

  Offset get handOffset => hand.offsetAt(now);

  bool get handMoving => hand.isMovingAt(now);

  void _onEvent(DeviceDemoEvent event) {
    switch (event) {
      case DeviceDemoEvent_Frame(:final field0):
        _pending = field0;
        if (!_decoding) _decodeNext();
      case DeviceDemoEvent_Gesture(:final field0):
        hand.cue(field0, now);
        tickHand();
        notifyListeners();
    }
  }

  // One decode at a time, always of the newest frame, so a slow decode can drop frames but can
  // never put an older screen up after a newer one.
  void _decodeNext() {
    final pixels = _pending;
    _pending = null;
    if (pixels == null || _disposed) return;
    _decoding = true;
    ui.decodeImageFromPixels(
      pixels,
      DeviceDemo.width(),
      DeviceDemo.height(),
      ui.PixelFormat.rgba8888,
      (image) {
        _decoding = false;
        if (_disposed) {
          image.dispose();
          return;
        }
        _first ??= image.clone();
        _screen?.dispose();
        _screen = image;
        notifyListeners();
        _decodeNext();
      },
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _events.cancel();
    _demo.dispose();
    handPlayer.dispose();
    _first?.dispose();
    _screen?.dispose();
    super.dispose();
  }
}
