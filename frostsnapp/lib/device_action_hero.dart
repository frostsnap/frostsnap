import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// What the full-screen device action dialog draws above its title for a caller that opts in.
class DeviceActionHeroSpec {
  const DeviceActionHeroSpec({
    required this.demo,
    this.changes,
    this.maxHeight = 430,
    this.chain = false,
  });

  /// The device's own screen for this action, played by a hand on the plugged-in device. It is
  /// read again whenever [changes] notifies, and is null while the device has nothing to show yet.
  final DeviceDemoKind? Function() demo;

  final Listenable? changes;

  /// Draw every connected device that still needs to act, in the order they are chained, rather
  /// than only the first.
  final bool chain;

  /// The hero never grows past this. It also shrinks to what the dialog's own content leaves
  /// free, so the content is never cut.
  final double maxHeight;
}

/// Devices in these case colours, daisy-chained from this phone or computer and seen from straight
/// above, playing [demo] with a hand.
class DeviceActionHero extends StatefulWidget {
  const DeviceActionHero({
    super.key,
    required this.caseColors,
    required this.demo,
    required this.maxHeight,
  });

  /// Never empty.
  final List<CaseColor?> caseColors;
  final DeviceDemoKind? demo;
  final double maxHeight;

  @override
  State<DeviceActionHero> createState() => _DeviceActionHeroState();
}

class _DeviceActionHeroState extends State<DeviceActionHero>
    with SingleTickerProviderStateMixin {
  DeviceDemoPlayback? _playback;
  final _handTick = ValueNotifier(0);
  late final Ticker _ticker;
  late final Future<Map<HandSheet, ui.Image>> _sheets;

  DeviceDemoPlayback? _play() {
    final demo = widget.demo;
    if (demo == null) return null;
    return DeviceDemoPlayback(demo, devices: widget.caseColors.length)
      ..addListener(_wakeHand);
  }

  bool get _handMoving => _playback?.handMoving ?? false;

  // Ticking only while the hand moves leaves the engine idle when nothing on the hero changes.
  void _wakeHand() {
    if (_handMoving && !_ticker.isActive) _ticker.start();
  }

  void _tickHand(Duration _) {
    _handTick.value++;
    if (!_handMoving) _ticker.stop();
  }

  @override
  void initState() {
    super.initState();
    _sheets = HandSheets.acquire();
    _ticker = createTicker(_tickHand);
    _playback = _play();
  }

  @override
  void didUpdateWidget(DeviceActionHero old) {
    super.didUpdateWidget(old);
    if (old.demo != widget.demo ||
        old.caseColors.length != widget.caseColors.length) {
      _playback?.dispose();
      _playback = _play();
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    _handTick.dispose();
    _playback?.dispose();
    HandSheets.release();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final playback = _playback;
    final devices = widget.caseColors.length;
    final scene = Platform.isAndroid || Platform.isIOS
        ? _phoneScene(devices)
        : _computerScene(devices);

    final chain = [
      for (final (i, caseColor) in widget.caseColors.indexed)
        _TopDownDevice(
          caseColor: caseColor,
          plugHidden: i > 0,
          screen: playback == null
              ? null
              : ListenableBuilder(
                  listenable: playback,
                  builder: (context, _) => RawImage(
                    image: playback.screen(i),
                    fit: BoxFit.fill,
                    filterQuality: FilterQuality.medium,
                  ),
                ),
        ),
    ];
    final hand = playback == null
        ? const SizedBox()
        : FutureBuilder(
            future: _sheets,
            builder: (context, sheets) => CustomPaint(
              painter: _HandPainter(playback, sheets.data, repaint: _handTick),
            ),
          );

    return LayoutBuilder(
      builder: (context, constraints) {
        final crop = scene.crop;
        final width = constraints.maxWidth;
        final height = [
          width * crop.height / crop.width,
          widget.maxHeight,
          constraints.maxHeight,
        ].reduce(math.min);
        Widget at(Offset position, Widget child) => Positioned(
          left: position.dx - crop.left,
          top: position.dy - crop.top,
          child: child,
        );
        // The shader mask fades but does not clip, so without this the arm draws over the
        // dialog's title.
        return ClipRect(
          child: SizedBox(
            width: width,
            height: height,
            child: ShaderMask(
              blendMode: BlendMode.dstIn,
              shaderCallback: scene.fade.createShader,
              child: FittedBox(
                child: SizedBox.fromSize(
                  size: crop.size,
                  child: Stack(
                    // A tall chain is narrower than the hero, and the arm reaches past the crop.
                    clipBehavior: Clip.none,
                    children: scene.layers(at, chain, hand),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _HandPainter extends CustomPainter {
  _HandPainter(this.playback, this.sheets, {required Listenable repaint})
    : super(repaint: repaint);

  final DeviceDemoPlayback playback;
  final Map<HandSheet, ui.Image>? sheets;

  @override
  void paint(Canvas canvas, Size size) {
    final frame = playback.handFrame;
    final sheet = frame == null ? null : sheets?[frame.sheet];
    if (frame == null || sheet == null) return;
    // Both scenes draw the hand in the plugged-in device's frame, where the chain runs down.
    final rect = Offset(0, playback.handDevice * _bodyLength) & size;
    canvas.saveLayer(rect, Paint());
    canvas.drawImageRect(
      sheet,
      frame.sheet.cell(frame.index),
      rect,
      Paint()..filterQuality = FilterQuality.medium,
    );
    // The clip's frame cuts the wrist off, which would show as an edge partway along a chain.
    canvas.drawRect(
      rect,
      Paint()
        ..blendMode = BlendMode.dstIn
        ..shader = const LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [Colors.black, Colors.transparent],
          stops: [0.55, 0.95],
        ).createShader(rect),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(_HandPainter old) =>
      old.playback != playback || old.sheets != sheets;
}

const _assets = 'assets/devices/topdown';

// The device render: 360×523 at about 10.3 px/mm, which sets the scale of both scenes.
const _deviceSize = Size(360, 523);
const _display = Rect.fromLTRB(33, 132, 326, 474);
const _displayRadius = 22.0;
final _deviceScreenSize = Size(
  DeviceDemo.width().toDouble(),
  DeviceDemo.height().toDouble(),
);
const _bodyTop = 84.0;
const _bodyLength = 434.0;

// The hand clips' frame, relative to the device, with the finger poised just below its display.
const _handOnDevice = Rect.fromLTWH(135, 134, 549.5, 897.1);

typedef _Place = Widget Function(Offset position, Widget child);

class _Scene {
  const _Scene({required this.crop, required this.fade, required this.layers});

  final Rect crop;
  final Gradient fade;
  final List<Widget> Function(_Place at, List<Widget> chain, Widget hand)
  layers;
}

_Scene _phoneScene(int devices) {
  const shellScale = 1.77;
  const shellSize = Size(560 * shellScale, 1120 * shellScale);
  const screen = Rect.fromLTWH(
    70 * shellScale + 18,
    109 * shellScale + 18,
    419 * shellScale - 36,
    902 * shellScale - 36,
  );
  const device = Offset(
    279.5 * shellScale - 180,
    1011 * shellScale + 3 - _bodyTop,
  );
  final crop = Rect.fromLTWH(200, 1560, 590, 860 + (devices - 1) * _bodyLength);
  return _Scene(
    crop: crop,
    // The hand comes up from below, so the bottom fades as well as the phone above.
    fade: LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: const [
        Colors.transparent,
        Colors.black,
        Colors.black,
        Colors.transparent,
      ],
      stops: [0, 138 / crop.height, 1 - 224 / crop.height, 1],
    ),
    layers: (at, chain, hand) => [
      for (final (i, link) in chain.indexed)
        at(device + Offset(0, i * _bodyLength), link),
      // The shell's screen is transparent. Covering it also hides the plug inside the phone.
      at(
        screen.topLeft,
        Container(
          width: screen.width,
          height: screen.height,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(80),
            gradient: const LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [Color(0xFF0C1116), Color(0xFF030507)],
            ),
          ),
        ),
      ),
      at(
        Offset.zero,
        Image.asset(
          '$_assets/phone.webp',
          width: shellSize.width,
          height: shellSize.height,
          fit: BoxFit.fill,
        ),
      ),
      at(
        device + _handOnDevice.topLeft,
        SizedBox.fromSize(size: _handOnDevice.size, child: hand),
      ),
    ],
  );
}

_Scene _computerScene(int devices) {
  const laptopScale = 1.236;
  const laptop = Offset(500, 0);
  const laptopEdge = 458 * laptopScale;
  const portY = 500 * laptopScale;
  // Turned a quarter so the plug points into the laptop's side.
  final centre = Offset(
    laptop.dx + laptopEdge - 3 - (_deviceSize.height / 2 - _bodyTop),
    portY,
  );
  Offset topLeft(Offset c) =>
      c - Offset(_deviceSize.height / 2, _deviceSize.width / 2);
  final chainLength = (devices - 1) * _bodyLength;
  final crop = Rect.fromLTWH(560 - chainLength, 330, 1100 + chainLength, 600);
  return _Scene(
    crop: crop,
    // The hand comes in from the left, along the chain.
    fade: LinearGradient(
      colors: const [Colors.transparent, Colors.black],
      stops: [0, 165 / crop.width],
    ),
    layers: (at, chain, hand) => [
      for (final (i, device) in chain.indexed)
        at(
          topLeft(centre - Offset(i * _bodyLength, 0)),
          RotatedBox(quarterTurns: 1, child: device),
        ),
      // Seen from above, the plug runs into the laptop's side under its top deck.
      at(
        laptop,
        Image.asset(
          '$_assets/laptop-edge.webp',
          width: 1500 * laptopScale,
          height: 1000 * laptopScale,
          fit: BoxFit.fill,
        ),
      ),
      // The clips are framed for the phone; turning them with the device keeps the finger on the
      // same part of its screen.
      at(
        centre - Offset(_deviceSize.width / 2, _deviceSize.height / 2),
        Transform.rotate(
          angle: math.pi / 2,
          child: SizedBox.fromSize(
            size: _deviceSize,
            child: Stack(
              clipBehavior: Clip.none,
              children: [Positioned.fromRect(rect: _handOnDevice, child: hand)],
            ),
          ),
        ),
      ),
    ],
  );
}

String _deviceAsset(CaseColor? caseColor) {
  final name = switch (caseColor) {
    CaseColor_Black() => 'black',
    CaseColor_Orange() => 'orange',
    CaseColor_Silver() => 'silver',
    CaseColor_Blue() => 'blue',
    CaseColor_Red() => 'red',
    CaseColor_Other() || null => 'plain',
  };
  return '$_assets/device-$name.webp';
}

class _TopDownDevice extends StatelessWidget {
  const _TopDownDevice({
    required this.caseColor,
    required this.screen,
    this.plugHidden = false,
  });

  final CaseColor? caseColor;
  final Widget? screen;

  /// A chained device's plug is inside the device upstream of it.
  final bool plugHidden;

  @override
  Widget build(BuildContext context) {
    final image = Image.asset(
      _deviceAsset(caseColor),
      width: _deviceSize.width,
      height: _deviceSize.height,
      fit: BoxFit.fill,
    );
    return SizedBox.fromSize(
      size: _deviceSize,
      child: Stack(
        children: [
          plugHidden ? ClipRect(clipper: _BelowPlug(), child: image) : image,
          Positioned.fromRect(
            rect: _display,
            child: ClipRRect(
              borderRadius: BorderRadius.circular(_displayRadius),
              child: FittedBox(
                fit: BoxFit.fill,
                child: SizedBox.fromSize(
                  size: _deviceScreenSize,
                  child: screen,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _BelowPlug extends CustomClipper<Rect> {
  @override
  Rect getClip(Size size) =>
      Rect.fromLTRB(0, _bodyTop, size.width, size.height);

  @override
  bool shouldReclip(_BelowPlug oldClipper) => false;
}
