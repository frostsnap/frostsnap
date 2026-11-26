import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// What the full-screen device action dialog draws above its title for a caller that opts in.
class DeviceActionHeroSpec {
  const DeviceActionHeroSpec({
    required this.demo,
    this.changes,
    this.maxHeight = 430,
    this.followChain = false,
  });

  /// The device's own screen for this action, played by a hand on the plugged-in device. It is
  /// read again whenever [changes] notifies, and is null while the device has nothing to show yet.
  final DeviceDemoKind? Function() demo;

  final Listenable? changes;

  /// Draw the whole connected chain, and move the camera down it to each device still to act.
  /// Without it, only the first device still to act is drawn.
  final bool followChain;

  /// The hero never grows past this. It also shrinks to what the dialog's own content leaves
  /// free, so the content is never cut.
  final double maxHeight;
}

/// Where a device in the chain stands on the dialog's action.
enum ChainRole { needed, done, notTakingPart }

typedef ChainDevice = ({DeviceId id, CaseColor? caseColor, ChainRole role});

/// A chain of devices plugged into this phone or computer, seen from straight above. The camera
/// is on the first device still needed, which plays [demo] with a hand.
class DeviceActionHero extends StatefulWidget {
  const DeviceActionHero({
    super.key,
    required this.chain,
    required this.demo,
    required this.maxHeight,
  });

  /// The hero [spec] draws over the connected [chain], or null when there's no device for it.
  ///
  /// A chain-following hero stays up while any participant is connected, so it can show the last
  /// confirmation while its dialog closes.
  static DeviceActionHero? of(
    DeviceActionHeroSpec spec,
    List<ChainDevice> chain,
  ) {
    final List<ChainDevice> drawn;
    if (spec.followChain) {
      if (chain.every((device) => device.role == ChainRole.notTakingPart)) {
        return null;
      }
      drawn = chain;
    } else {
      final needed = chain.where((device) => device.role == ChainRole.needed);
      if (needed.isEmpty) return null;
      drawn = [needed.first];
    }
    return DeviceActionHero(
      chain: drawn,
      demo: spec.demo(),
      maxHeight: spec.maxHeight,
    );
  }

  /// In chain order, the device plugged into the host first.
  final List<ChainDevice> chain;
  final DeviceDemoKind? demo;
  final double maxHeight;

  DeviceId? get _neededFirst =>
      chain.where((device) => device.role == ChainRole.needed).firstOrNull?.id;

  @override
  State<DeviceActionHero> createState() => _DeviceActionHeroState();
}

class _DeviceActionHeroState extends State<DeviceActionHero>
    with TickerProviderStateMixin {
  DeviceDemoPlayback? _playback;
  final _handTick = ValueNotifier(0);
  late final Ticker _ticker;

  /// Kept once no device is needed, so the camera stays on the last confirmation while the
  /// dialog closes.
  DeviceId? _focus;

  /// Where the camera is, in devices down the chain.
  late final AnimationController _camera;
  int _cameraTarget = 0;

  /// The screen the demo ends on, which a done device keeps showing, as the real one does.
  ui.Image? _end;
  DeviceDemoKind? _endFor;

  @visibleForTesting
  DeviceDemoPlayback? get playback => _playback;

  @visibleForTesting
  ui.Image? get endScreen => _end;

  // Rendering the end takes a whole play of the demo, so only a hero with a done device pays for it.
  void _ensureEnd() {
    final demo = widget.demo;
    final wanted = widget.chain.any((device) => device.role == ChainRole.done)
        ? demo
        : _endFor;
    if (wanted == _endFor) return;
    _endFor = wanted;
    _end?.dispose();
    _end = null;
    if (wanted == null) return;
    ui.decodeImageFromPixels(
      DeviceDemo.endScreen(kind: wanted),
      DeviceDemo.width(),
      DeviceDemo.height(),
      ui.PixelFormat.rgba8888,
      (image) {
        if (!mounted || _endFor != wanted) {
          image.dispose();
          return;
        }
        setState(() => _end = image);
      },
    );
  }

  int? _indexOf(DeviceId? id) {
    final i = widget.chain.indexWhere((device) => device.id == id);
    return i < 0 ? null : i;
  }

  DeviceDemoPlayback? _play() {
    final demo = widget.demo;
    if (demo == null) return null;
    return DeviceDemoPlayback(demo)..addListener(_wakeHand);
  }

  void _replay() {
    _playback?.dispose();
    _playback = _play();
  }

  bool get _handMoving => _playback?.handMoving ?? false;

  // Ticking only while the hand moves leaves the engine idle when nothing on the hero changes.
  void _wakeHand() {
    if (_handMoving && !_ticker.isActive) _ticker.start();
  }

  void _tickHand(Duration _) {
    _playback?.tickHand();
    _handTick.value++;
    if (!_handMoving) _ticker.stop();
  }

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_tickHand);
    _focus = widget._neededFirst ?? widget.chain.firstOrNull?.id;
    _cameraTarget = _indexOf(_focus) ?? 0;
    _camera = AnimationController.unbounded(
      vsync: this,
      value: _cameraTarget.toDouble(),
    );
    _playback = _play();
    _ensureEnd();
  }

  @override
  void didUpdateWidget(DeviceActionHero old) {
    super.didUpdateWidget(old);
    _ensureEnd();
    final focus = widget._neededFirst ?? _focus;
    final confirmed =
        focus != _focus &&
        widget.chain.any(
          (device) => device.id == _focus && device.role == ChainRole.done,
        );
    if (focus != _focus || old.demo != widget.demo) {
      _focus = focus;
      _replay();
    }
    final index = _indexOf(_focus);
    if (index == null) return;
    // Only a confirmation is a move along the chain. A device plugged or unplugged around the
    // focus changes where it is drawn, not what the user is looking at, so the frame follows it.
    if (confirmed) {
      _cameraTarget = index;
      _camera.animateTo(
        index.toDouble(),
        duration: const Duration(milliseconds: 600),
        curve: Curves.easeInOutCubic,
      );
    } else if (index != _cameraTarget) {
      _cameraTarget = index;
      _camera.value = index.toDouble();
    }
  }

  @override
  void dispose() {
    _ticker.dispose();
    _camera.dispose();
    _handTick.dispose();
    _playback?.dispose();
    _end?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final playback = _playback;
    final chain = widget.chain;
    final scene = Platform.isAndroid || Platform.isIOS
        ? _phoneScene()
        : _computerScene(chain.length > 1);

    Widget? screen(ui.Image? Function(DeviceDemoPlayback) image) =>
        playback == null
        ? null
        : ListenableBuilder(
            listenable: playback,
            builder: (context, _) => RawImage(
              image: image(playback),
              fit: BoxFit.fill,
              filterQuality: FilterQuality.medium,
            ),
          );
    final devices = [
      for (final (i, device) in chain.indexed)
        _TopDownDevice(
          key: ValueKey(device.id),
          caseColor: device.caseColor,
          plugHidden: i > 0,
          screen: switch (device.role) {
            ChainRole.needed when device.id == _focus => screen(
              (playback) => playback.screen,
            ),
            ChainRole.needed => screen((playback) => playback.firstScreen),
            ChainRole.done => switch (_end) {
              final end? => RawImage(
                image: end,
                fit: BoxFit.fill,
                filterQuality: FilterQuality.medium,
              ),
              null => null,
            },
            ChainRole.notTakingPart => null,
          },
        ),
    ];
    final focusIndex = _indexOf(_focus);
    final handOn =
        focusIndex != null && chain[focusIndex].role == ChainRole.needed
        ? focusIndex
        : null;
    final hand = playback == null || handOn == null
        ? const SizedBox()
        : CustomPaint(
            key: const ValueKey('hand'),
            painter: _HandPainter(
              playback,
              repaint: Listenable.merge([_handTick, playback.handPlayer]),
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
                  child: AnimatedBuilder(
                    animation: _camera,
                    builder: (context, child) => Transform.translate(
                      offset: -scene.step * _camera.value,
                      child: child,
                    ),
                    child: Stack(
                      // The arm reaches past the crop where the hero is wider than it.
                      clipBehavior: Clip.none,
                      children: scene.layers(at, devices, handOn ?? 0, hand),
                    ),
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
  _HandPainter(this.playback, {required Listenable repaint})
    : super(repaint: repaint);

  final DeviceDemoPlayback playback;

  @override
  void paint(Canvas canvas, Size size) {
    final image = playback.handPlayer.image;
    if (image == null) return;
    final offset = playback.handOffset;
    canvas.drawImageRect(
      image,
      Offset.zero & Size(image.width.toDouble(), image.height.toDouble()),
      Offset(
            offset.dx * _display.width / _deviceScreenSize.width,
            offset.dy * _display.height / _deviceScreenSize.height,
          ) &
          size,
      Paint()..filterQuality = FilterQuality.medium,
    );
  }

  @override
  bool shouldRepaint(_HandPainter old) => old.playback != playback;
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
  const _Scene({
    required this.crop,
    required this.fade,
    required this.step,
    required this.layers,
  });

  final Rect crop;
  final Gradient fade;

  /// From one device in the chain to the next.
  final Offset step;
  final List<Widget> Function(
    _Place at,
    List<Widget> devices,
    int focus,
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
  const step = Offset(0, _bodyLength);
  return _Scene(
    crop: const Rect.fromLTWH(200, 1560, 590, 860),
    step: step,
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
    layers: (at, devices, focus, hand) => [
      for (final (i, child) in devices.indexed)
        at(device + step * i.toDouble(), child),
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
        device + step * focus.toDouble() + _handOnDevice.topLeft,
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
  const step = Offset(-_bodyLength, 0);
  return _Scene(
    step: step,
    crop: chained
        ? const Rect.fromLTWH(420, 330, 1240, 600)
        : const Rect.fromLTWH(560, 330, 1100, 600),
    // The hand comes in from the left, as does the chain.
    fade: LinearGradient(
      colors: const [Colors.transparent, Colors.black],
      stops: [0, chained ? 0.3 : 0.15],
    ),
    layers: (at, devices, focus, hand) => [
      for (final (i, child) in devices.indexed)
        at(
          topLeft(centre + step * i.toDouble()),
          RotatedBox(quarterTurns: 1, child: child),
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
        centre +
            step * focus.toDouble() -
            Offset(_deviceSize.width / 2, _deviceSize.height / 2),
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
    super.key,
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
