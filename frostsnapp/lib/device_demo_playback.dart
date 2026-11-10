import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/animation.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

enum HandSheet {
  swipe('assets/devices/hand/hand-swipe-sheet.webp'),
  hold('assets/devices/hand/hand-hold-sheet.webp');

  const HandSheet(this.asset);

  final String asset;

  static const columns = 20;
  static const cellSize = ui.Size(186, 304);

  // The wrist runs off a cell's edge, so cells packed edge to edge bleed into each other's
  // edges when filtered. The sheets leave this much transparency around every cell.
  static const _margin = 8.0;

  ui.Rect cell(int index) => ui.Rect.fromLTWH(
    (index % columns) * (cellSize.width + 2 * _margin) + _margin,
    (index ~/ columns) * (cellSize.height + 2 * _margin) + _margin,
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
  static const _holdLiftFrame = 137;
  static const _holdFrames = 210;

  /// Where on the device's screen every clip puts the fingertip. A tap elsewhere moves the hand.
  static const fingertip = Offset(120, 215);

  // A tap is the hold clip's own reach, press and lift, played quickly: the same hand, lit and
  // framed the same, with no sheet of its own. Hold frames up to 22 and from 171 are still the
  // rest pose, so a tap starts and ends where every clip does.
  static final List<HandFrame> _tapClip = [
    for (var i = 20; i < holdContactFrame; i += 2) HandFrame(HandSheet.hold, i),
    for (var i = 0; i < _tapPressFrames; i++)
      const HandFrame(HandSheet.hold, holdContactFrame),
    for (var i = _holdLiftFrame; i <= 171; i += 2) HandFrame(HandSheet.hold, i),
  ];
  static final tapContactFrame = _tapClip.indexOf(
    const HandFrame(HandSheet.hold, holdContactFrame),
  );
  static const _tapPressFrames = 3;

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

  static int _ms(int frames) => (frames * 1000 / fps).round();

  /// The clips' timing, which the device demo scripts its touches by.
  static final timing = DeviceDemoHandTiming(
    swipeContactMs: _ms(swipeContactFrame),
    swipeAfterContactMs: _ms(_swipeCycle.length - swipeContactFrame),
    holdContactMs: _ms(holdContactFrame),
    holdPressMs: _ms(_holdLiftFrame - holdContactFrame),
    holdAfterReleaseMs: _ms(_holdFrames - _holdLiftFrame),
    tapContactMs: _ms(tapContactFrame),
    tapPressMs: _ms(_tapPressFrames),
    tapAfterReleaseMs: _ms(_tapClip.length - tapContactFrame - _tapPressFrames),
  );

  Duration? _start;
  List<HandFrame> _clip = const [];
  ({Duration at, Offset offset}) _from = (
    at: Duration.zero,
    offset: Offset.zero,
  );
  ({Duration at, Offset offset}) _to = (at: Duration.zero, offset: Offset.zero);

  void cue(DeviceDemoCue cue, Duration at) {
    final (clip, contactFrame) = switch (cue.gesture) {
      DeviceDemoGesture_Swipe() => (
        [for (final i in _swipeCycle) HandFrame(HandSheet.swipe, i)],
        swipeContactFrame,
      ),
      DeviceDemoGesture_Hold(:final holdMs) => (
        _holdClip(holdMs),
        holdContactFrame,
      ),
      DeviceDemoGesture_Tap() => (_tapClip, tapContactFrame),
    };
    final contact = at + Duration(milliseconds: cue.contactInMs);
    _from = (at: at, offset: offsetAt(at));
    _to = (
      at: contact,
      offset: switch (cue.gesture) {
        DeviceDemoGesture_Tap(:final x, :final y) =>
          Offset(x.toDouble(), y.toDouble()) - fingertip,
        _ => Offset.zero,
      },
    );
    _clip = clip;
    // The touch lands on a whole millisecond, so put it mid-frame: either side of the frame's
    // edge it would show the frame before or after contact.
    _start =
        contact -
        Duration(microseconds: ((contactFrame + 0.5) * 1000000 / fps).round());
  }

  /// Null until the first gesture: a demo that never asks for one has no hand.
  HandFrame? frameAt(Duration now) {
    final start = _start;
    if (start == null) return null;
    final n = ((now - start).inMicroseconds * fps / 1000000).floor();
    if (n < 0 || n >= _clip.length) return HandFrame.rest;
    return _clip[n];
  }

  /// How far from [fingertip] the hand is, in the device screen's pixels. It moves to each tap's
  /// target as the finger reaches for it.
  Offset offsetAt(Duration now) {
    if (now >= _to.at) return _to.offset;
    if (now <= _from.at) return _from.offset;
    final t = Curves.easeInOut.transform(
      (now - _from.at).inMicroseconds / (_to.at - _from.at).inMicroseconds,
    );
    return Offset.lerp(_from.offset, _to.offset, t) ?? _to.offset;
  }

  /// Whether the hand is still playing its clip.
  bool isMovingAt(Duration now) {
    final start = _start;
    return start != null &&
        now < start + Duration(microseconds: _clip.length * 1000000 ~/ fps);
  }

  // The hand is motionless mid-press, so a longer hold repeats that one frame.
  static List<HandFrame> _holdClip(int holdMs) {
    final extra =
        ((holdMs * fps / 1000).round() - (_holdLiftFrame - holdContactFrame))
            .clamp(0, 1 << 20);
    return [
      for (var i = 0; i < _holdStillFrame; i++) HandFrame(HandSheet.hold, i),
      for (var i = 0; i < extra; i++)
        HandFrame(HandSheet.hold, _holdStillFrame),
      for (var i = _holdStillFrame; i < _holdFrames; i++)
        HandFrame(HandSheet.hold, i),
    ];
  }
}

/// A running device demo: its latest screen and where the hand is.
class DeviceDemoPlayback extends ChangeNotifier {
  DeviceDemoPlayback(DeviceDemoKind kind)
    : _demo = DeviceDemo(kind: kind, hand: HandTrack.timing) {
    _clock.start();
    _events = _demo.events().listen(_onEvent);
  }

  final DeviceDemo _demo;
  late final StreamSubscription<DeviceDemoEvent> _events;
  final _clock = Stopwatch();
  final hand = HandTrack();
  ui.Image? _first;
  ui.Image? _screen;
  bool _decoding = false;
  Uint8List? _pending;
  bool _disposed = false;

  ui.Image? get screen => _screen;

  /// The screen the demo opened on, which a device the hand never touches keeps showing.
  ui.Image? get firstScreen => _first;

  Duration get now => _clock.elapsed;

  HandFrame? get handFrame => hand.frameAt(now);

  Offset get handOffset => hand.offsetAt(now);

  bool get handMoving => hand.isMovingAt(now);

  void _onEvent(DeviceDemoEvent event) {
    switch (event) {
      case DeviceDemoEvent_Frame(:final field0):
        _pending = field0;
        if (!_decoding) _decodeNext();
      case DeviceDemoEvent_Gesture(:final field0):
        hand.cue(field0, now);
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
    _first?.dispose();
    _screen?.dispose();
    super.dispose();
  }
}

/// The decoded hand sheets, held only while a hero is showing: decoded they take tens of MB.
class HandSheets {
  static int _holders = 0;
  static Future<Map<HandSheet, ui.Image>>? _loaded;

  static Future<Map<HandSheet, ui.Image>> acquire() {
    _holders++;
    return _loaded ??= _load();
  }

  static void release() {
    _holders--;
    if (_holders > 0) return;
    final loaded = _loaded;
    _loaded = null;
    loaded?.then((sheets) {
      for (final image in sheets.values) {
        image.dispose();
      }
    });
  }

  static Future<Map<HandSheet, ui.Image>> _load() async {
    final entries = await Future.wait(
      HandSheet.values.map((sheet) async {
        final data = await rootBundle.load(sheet.asset);
        final codec = await ui.instantiateImageCodec(data.buffer.asUint8List());
        try {
          return MapEntry(sheet, (await codec.getNextFrame()).image);
        } finally {
          codec.dispose();
        }
      }),
    );
    return Map.fromEntries(entries);
  }
}
