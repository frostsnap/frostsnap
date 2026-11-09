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
  );

  Duration? _start;
  List<HandFrame> _clip = const [];
  ({Duration at, int device})? _leaving;
  int _device = 0;

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
    };
    _leaving = _start == null ? null : (at: at, device: _device);
    _device = cue.device;
    _clip = clip;
    _start =
        at +
        Duration(milliseconds: cue.contactInMs) -
        Duration(microseconds: contactFrame * 1000000 ~/ fps);
  }

  /// Where along the chain the hand is, in devices from the one plugged in. A cue that leads its
  /// clip by more than the clip's own approach leaves the hand that long to get to the device.
  double deviceAt(Duration now) {
    final leaving = _leaving;
    final start = _start;
    if (leaving == null || start == null || now >= start) {
      return _device.toDouble();
    }
    final travel = Curves.easeInOut.transform(
      ((now - leaving.at).inMicroseconds / (start - leaving.at).inMicroseconds)
          .clamp(0, 1),
    );
    return leaving.device + (_device - leaving.device) * travel;
  }

  /// Null until the first gesture: a demo that never asks for one has no hand.
  HandFrame? frameAt(Duration now) {
    final start = _start;
    if (start == null) return null;
    final n = ((now - start).inMicroseconds * fps / 1000000).floor();
    if (n < 0 || n >= _clip.length) return HandFrame.rest;
    return _clip[n];
  }

  /// Whether the hand is still going: travelling to its device or playing its clip.
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

/// A running device demo: the latest screen of each device in the chain and where the hand is.
class DeviceDemoPlayback extends ChangeNotifier {
  DeviceDemoPlayback(DeviceDemoKind kind, {required int devices})
    : this._(DeviceDemo(kind: kind, devices: devices, hand: HandTrack.timing));

  DeviceDemoPlayback._(this._demo)
    : _screens = List.filled(_demo.devices(), null) {
    _clock.start();
    _events = _demo.events().listen(_onEvent);
  }

  final DeviceDemo _demo;
  late final StreamSubscription<DeviceDemoEvent> _events;
  final _clock = Stopwatch();
  final hand = HandTrack();
  final List<ui.Image?> _screens;
  bool _decoding = false;
  final _pending = <int, Uint8List>{};
  bool _disposed = false;

  ui.Image? screen(int device) =>
      device < _screens.length ? _screens[device] : null;

  Duration get now => _clock.elapsed;

  HandFrame? get handFrame => hand.frameAt(now);

  double get handDevice => hand.deviceAt(now);

  bool get handMoving => hand.isMovingAt(now);

  void _onEvent(DeviceDemoEvent event) {
    switch (event) {
      case DeviceDemoEvent_Frame(:final device, :final pixels):
        _pending[device] = pixels;
        if (!_decoding) _decodeNext();
      case DeviceDemoEvent_Gesture(:final field0):
        hand.cue(field0, now);
        notifyListeners();
    }
  }

  // One decode at a time, always of a device's newest frame, so a slow decode can drop frames but
  // can never put an older screen up after a newer one.
  void _decodeNext() {
    if (_pending.isEmpty || _disposed) return;
    final MapEntry(key: device, value: pixels) = _pending.entries.first;
    _pending.remove(device);
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
        _screens[device]?.dispose();
        _screens[device] = image;
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
    for (final screen in _screens) {
      screen?.dispose();
    }
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
