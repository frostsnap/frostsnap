import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

enum HandSheet {
  swipe('assets/devices/hand/hand-swipe-sheet.webp', columns: 10),
  hold('assets/devices/hand/hand-hold-sheet.webp', columns: 15);

  const HandSheet(this.asset, {required this.columns});

  final String asset;
  final int columns;

  static const cellSize = ui.Size(248, 405);

  ui.Rect cell(int index) => ui.Rect.fromLTWH(
    (index % columns) * cellSize.width,
    (index ~/ columns) * cellSize.height,
    cellSize.width,
    cellSize.height,
  );
}

@immutable
class HandFrame {
  const HandFrame(this.sheet, this.index);

  final HandSheet sheet;
  final int index;

  /// Both clips start and end on this pose, so the hand can wait in it between gestures.
  static const rest = HandFrame(HandSheet.swipe, 0);

  @override
  bool operator ==(Object other) =>
      other is HandFrame && other.sheet == sheet && other.index == index;

  @override
  int get hashCode => Object.hash(sheet, index);
}

/// Which frame of the hand clips to draw, given the gestures the device demo announced.
class HandTrack {
  static const fps = 30;
  static const swipeContactFrame = 30;
  static const holdContactFrame = 45;
  static const _holdStillFrame = 90;
  static const _holdPressedFrames = 137 - holdContactFrame;

  // The finger's upward stroke (clip frames 30–55) plays at double speed; the frames saved rest on
  // the end pose so the cycle keeps its length.
  static final List<int> _swipeCycle = () {
    final stroke = [for (var i = 30; i <= 55; i += 2) i];
    final after = [for (var i = 56; i <= 94; i++) i];
    final rest = 95 - (30 + stroke.length + after.length);
    return [
      for (var i = 0; i < 30; i++) i,
      ...stroke,
      ...after,
      for (var i = 0; i < rest; i++) 94,
    ];
  }();

  Duration? _start;
  List<HandFrame> _clip = const [];

  void cue(DeviceDemoEvent event, Duration at) {
    final (clip, contactFrame, contactInMs) = switch (event) {
      DeviceDemoEvent_Swipe(:final contactInMs) => (
        [for (final i in _swipeCycle) HandFrame(HandSheet.swipe, i)],
        swipeContactFrame,
        contactInMs,
      ),
      DeviceDemoEvent_Hold(:final contactInMs, :final holdMs) => (
        _holdClip(holdMs),
        holdContactFrame,
        contactInMs,
      ),
      DeviceDemoEvent_Frame() => throw ArgumentError('not a gesture'),
    };
    _clip = clip;
    _start =
        at +
        Duration(milliseconds: contactInMs) -
        Duration(microseconds: contactFrame * 1000000 ~/ fps);
  }

  /// Null until the first gesture: a demo that never asks for one has no hand.
  HandFrame? frameAt(Duration now) {
    final start = _start;
    if (start == null) return null;
    final n = ((now - start).inMicroseconds * fps / 1000000).floor();
    if (n < 0 || n >= _clip.length) return HandFrame.rest;
    return _clip[n];
  }

  // The hand is motionless mid-press, so a longer hold repeats that one frame.
  static List<HandFrame> _holdClip(int holdMs) {
    final extra = ((holdMs * fps / 1000).round() - _holdPressedFrames).clamp(
      0,
      1 << 20,
    );
    return [
      for (var i = 0; i < _holdStillFrame; i++) HandFrame(HandSheet.hold, i),
      for (var i = 0; i < extra; i++)
        HandFrame(HandSheet.hold, _holdStillFrame),
      for (var i = _holdStillFrame; i < 210; i++) HandFrame(HandSheet.hold, i),
    ];
  }
}

/// A running device demo: its latest screen and where the hand is.
class DeviceDemoPlayback extends ChangeNotifier {
  DeviceDemoPlayback(DeviceDemoKind kind) : _demo = DeviceDemo(kind: kind) {
    _clock.start();
    _events = _demo.events().listen(_onEvent);
  }

  final DeviceDemo _demo;
  late final StreamSubscription<DeviceDemoEvent> _events;
  final _clock = Stopwatch();
  final hand = HandTrack();
  ui.Image? _screen;
  bool _decoding = false;
  Uint8List? _pending;
  bool _disposed = false;

  ui.Image? get screen => _screen;

  Duration get now => _clock.elapsed;

  HandFrame? get handFrame => hand.frameAt(now);

  void _onEvent(DeviceDemoEvent event) {
    switch (event) {
      case DeviceDemoEvent_Frame(:final field0):
        _pending = field0;
        if (!_decoding) _decodeNext();
      case DeviceDemoEvent_Swipe() || DeviceDemoEvent_Hold():
        hand.cue(event, now);
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
    _screen?.dispose();
    super.dispose();
  }
}

final Future<Map<HandSheet, ui.Image>> handSheets = () async {
  final entries = await Future.wait(
    HandSheet.values.map((sheet) async {
      final data = await rootBundle.load(sheet.asset);
      final codec = await ui.instantiateImageCodec(data.buffer.asUint8List());
      return MapEntry(sheet, (await codec.getNextFrame()).image);
    }),
  );
  return Map.fromEntries(entries);
}();
