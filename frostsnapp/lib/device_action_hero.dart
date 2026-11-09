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
    this.maxHeight = 430,
    this.showNext = false,
  });

  /// The device's own screen for this action, played by a hand on the plugged-in device.
  final DeviceDemoKind demo;

  /// Draw the next needed device in the chain too. It shows the same screen, as every device
  /// does at once in an action like the security check.
  final bool showNext;

  /// The hero never grows past this. It also shrinks to what the dialog's own content leaves
  /// free, so the content is never cut.
  final double maxHeight;
}

/// A device in [caseColor] plugged into this phone or computer, seen from straight above, playing
/// [demo] with a hand, with [next] (if any) daisy-chained behind it.
class DeviceActionHero extends StatefulWidget {
  const DeviceActionHero({
    super.key,
    required this.caseColor,
    required this.demo,
    required this.maxHeight,
    this.next,
  });

  final CaseColor? caseColor;
  final DeviceDemoKind demo;
  final double maxHeight;
  final ({CaseColor? caseColor})? next;

  @override
  State<DeviceActionHero> createState() => _DeviceActionHeroState();
}

class _DeviceActionHeroState extends State<DeviceActionHero>
    with SingleTickerProviderStateMixin {
  late DeviceDemoPlayback _playback;
  final _handTick = ValueNotifier(0);
  late final Ticker _ticker;

  @override
  void initState() {
    super.initState();
    _playback = DeviceDemoPlayback(widget.demo);
    _ticker = createTicker((_) => _handTick.value++)..start();
  }

  @override
  void didUpdateWidget(DeviceActionHero old) {
    super.didUpdateWidget(old);
    if (old.demo != widget.demo) {
      _playback.dispose();
      _playback = DeviceDemoPlayback(widget.demo);
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    _handTick.dispose();
    _playback.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final next = widget.next;
    final chained = next != null;
    final scene = Platform.isAndroid || Platform.isIOS
        ? _phoneScene()
        : _computerScene(chained);

    final screen = ListenableBuilder(
      listenable: _playback,
      builder: (context, _) => RawImage(
        image: _playback.screen,
        fit: BoxFit.fill,
        filterQuality: FilterQuality.medium,
      ),
    );
    final first = _TopDownDevice(caseColor: widget.caseColor, screen: screen);
    final second = next == null
        ? null
        : _TopDownDevice(
            caseColor: next.caseColor,
            screen: screen,
            plugHidden: true,
          );
    final hand = FutureBuilder(
      future: handSheets,
      builder: (context, sheets) => CustomPaint(
        painter: _HandPainter(_playback, sheets.data, repaint: _handTick),
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
        return SizedBox(
          width: width,
          height: height,
          child: ShaderMask(
            blendMode: BlendMode.dstIn,
            shaderCallback: scene.fade.createShader,
            child: FittedBox(
              child: SizedBox.fromSize(
                size: crop.size,
                child: Stack(
                  clipBehavior: Clip.hardEdge,
                  children: scene.layers(at, first, second, hand),
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
    canvas.drawImageRect(
      sheet,
      frame.sheet.cell(frame.index),
      Offset.zero & size,
      Paint()..filterQuality = FilterQuality.medium,
    );
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
  final List<Widget> Function(
    _Place at,
    Widget first,
    Widget? second,
    Widget hand,
  )
  layers;
}

_Scene _phoneScene() {
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
  return _Scene(
    crop: const Rect.fromLTWH(200, 1560, 590, 860),
    // The hand comes up from below, so the bottom fades as well as the phone above.
    fade: const LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: [
        Colors.transparent,
        Colors.black,
        Colors.black,
        Colors.transparent,
      ],
      stops: [0, 0.16, 0.74, 1],
    ),
    layers: (at, first, second, hand) => [
      at(device, first),
      if (second != null) at(device + const Offset(0, _bodyLength), second),
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
        ),
      ),
      at(
        device + _handOnDevice.topLeft,
        SizedBox.fromSize(size: _handOnDevice.size, child: hand),
      ),
    ],
  );
}

_Scene _computerScene(bool chained) {
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
  return _Scene(
    crop: chained
        ? const Rect.fromLTWH(420, 330, 1240, 600)
        : const Rect.fromLTWH(560, 330, 1100, 600),
    // The hand comes in from the left, as does the chain.
    fade: LinearGradient(
      colors: const [Colors.transparent, Colors.black],
      stops: [0, chained ? 0.3 : 0.15],
    ),
    layers: (at, first, second, hand) => [
      at(topLeft(centre), RotatedBox(quarterTurns: 1, child: first)),
      if (second != null)
        at(
          topLeft(centre - const Offset(_bodyLength, 0)),
          RotatedBox(quarterTurns: 1, child: second),
        ),
      // Seen from above, the plug runs into the laptop's side under its top deck.
      at(
        laptop,
        Image.asset(
          '$_assets/laptop-edge.webp',
          width: 1500 * laptopScale,
          height: 1000 * laptopScale,
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
  final Widget screen;

  /// A chained device's plug is inside the device upstream of it.
  final bool plugHidden;

  @override
  Widget build(BuildContext context) {
    final image = Image.asset(
      _deviceAsset(caseColor),
      width: _deviceSize.width,
      height: _deviceSize.height,
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
