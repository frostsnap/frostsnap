import 'dart:async';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_hero.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/lib.dart';

import 'support/rust_lib.dart';

DeviceId _id(int n) => DeviceId(field0: U8Array33(Uint8List(33)..[0] = n));

final _hero = DeviceActionHero(
  chain: [
    (id: _id(1), caseColor: const CaseColor_Orange(), role: ChainRole.needed),
  ],
  demo: const DeviceDemoKind.sign(),
  maxHeight: 430,
);

final _keygen = DeviceDemoKind.keygen(
  threshold: 2,
  nDevices: 3,
  securityCheckCode: U8Array4(Uint8List.fromList([1, 2, 3, 4])),
);

const _colors = [
  CaseColor_Orange(),
  CaseColor_Blue(),
  CaseColor_Red(),
  CaseColor_Silver(),
];

final _keygenSpec = DeviceActionHeroSpec(
  demo: () => _keygen,
  followChain: true,
);

/// Devices by number: each keeps its colour wherever it sits in the chain.
List<ChainDevice> _chainOf(List<(int, ChainRole)> devices) => [
  for (final (n, role) in devices)
    (id: _id(n), caseColor: _colors[n - 1], role: role),
];

Widget _frame(Widget? hero) =>
    MaterialApp(home: SizedBox(width: 800, child: hero));

Widget _chainHero(List<(int, ChainRole)> devices) =>
    _frame(DeviceActionHero.of(_keygenSpec, _chainOf(devices)));

Finder _device(int n) => find.byKey(ValueKey(_id(n)));

Offset _at(WidgetTester tester, int n) => tester.getCenter(_device(n));

String _asset(WidgetTester tester, int n) {
  final image = tester.widget<Image>(
    find.descendant(of: _device(n), matching: find.byType(Image)).first,
  );
  return (image.image as AssetImage).assetName;
}

bool _checked(int n) => find
    .descendant(of: _device(n), matching: find.byIcon(Icons.check_circle))
    .evaluate()
    .isNotEmpty;

final _hand = find.byKey(const ValueKey('hand'));

Object? _playback(WidgetTester tester) =>
    (tester.state(find.byType(DeviceActionHero)) as dynamic).playback;

// Past the camera's ease, without settling: the hand keeps scheduling frames.
Future<void> _pan(WidgetTester tester) =>
    tester.pump(const Duration(milliseconds: 700));

const _needed = ChainRole.needed;
const _done = ChainRole.done;
const _stranger = ChainRole.notTakingPart;

void main() {
  setUpAll(initRustLibForTests);

  testWidgets('a hero with nothing to play leaves the engine idle', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: SizedBox(
          width: 400,
          child: DeviceActionHero(
            chain: [
              (
                id: _id(1),
                caseColor: const CaseColor_Orange(),
                role: ChainRole.needed,
              ),
            ],
            demo: null,
            maxHeight: 430,
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 100));
    expect(tester.binding.hasScheduledFrame, isFalse);

    await tester.pumpWidget(const SizedBox());
  });

  for (final compact in [true, false]) {
    testWidgets('the hero gives way to the dialog content in a short window '
        '(compact: $compact)', (tester) async {
      tester.view.physicalSize = const Size(600, 420);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      await tester.pumpWidget(
        MaterialApp(
          home: MediaQuery(
            data: const MediaQueryData(
              size: Size(600, 420),
              textScaler: TextScaler.linear(1.5),
            ),
            child: Center(
              child: DeviceActionDialogLayout(
                compact: compact,
                picture: _hero,
                title: 'Sign transaction with connected device',
                actionButtons: [
                  OutlinedButton(onPressed: () {}, child: const Text('Cancel')),
                ],
              ),
            ),
          ),
        ),
      );

      expect(tester.takeException(), isNull);
      expect(find.text('Cancel').hitTestable(), findsOneWidget);
      expect(
        find.text('Sign transaction with connected device').hitTestable(),
        findsOneWidget,
      );
      expect(
        tester.getSize(find.byType(DeviceActionHero)).height,
        lessThan(_hero.maxHeight / 2),
      );

      await tester.pumpWidget(const SizedBox());
    });
  }

  test(
    'a device the hand never touches stays on the screen it opened on',
    () async {
      final playback = DeviceDemoPlayback(
        DeviceDemoKind.keygen(
          threshold: 2,
          nDevices: 3,
          securityCheckCode: U8Array4(Uint8List.fromList([1, 2, 3, 4])),
        ),
      );
      ui.Image? openedOn;
      final changed = Completer<void>();
      playback.addListener(() {
        final screen = playback.screen;
        openedOn ??= screen;
        if (screen != openedOn && !changed.isCompleted) changed.complete();
      });
      await changed.future.timeout(const Duration(seconds: 10));
      final first = playback.firstScreen;
      await Future.delayed(const Duration(milliseconds: 500));

      expect(playback.firstScreen, same(first));
      expect(playback.screen, isNot(same(first)));
      playback.dispose();
    },
  );

  testWidgets('a confirmed device stays in its place and colour, and the '
      'camera moves on to the next one', (tester) async {
    await tester.pumpWidget(
      _chainHero([(1, _needed), (2, _needed), (3, _needed)]),
    );
    final focus = _at(tester, 1);
    final step = _at(tester, 2) - focus;
    final firstAsset = _asset(tester, 1);

    await tester.pumpWidget(
      _chainHero([(1, _done), (2, _needed), (3, _needed)]),
    );
    await _pan(tester);

    expect(_device(1), findsOneWidget);
    expect(_asset(tester, 1), firstAsset);
    expect(_checked(1), isTrue);
    expect(_at(tester, 2), offsetMoreOrLessEquals(focus));
    expect(_at(tester, 1), offsetMoreOrLessEquals(focus - step));

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a device confirmed out of order is done, and the camera stays', (
    tester,
  ) async {
    await tester.pumpWidget(
      _chainHero([(1, _needed), (2, _needed), (3, _needed)]),
    );
    final focus = _at(tester, 1);
    final playing = _playback(tester);

    await tester.pumpWidget(
      _chainHero([(1, _needed), (2, _done), (3, _needed)]),
    );
    await _pan(tester);

    expect(_at(tester, 1), offsetMoreOrLessEquals(focus));
    expect(_checked(2), isTrue);
    expect(_checked(1), isFalse);
    expect(_playback(tester), same(playing));

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('the camera moves by the chain distance to the next device '
      'still needed', (tester) async {
    await tester.pumpWidget(
      _chainHero([(1, _needed), (2, _done), (3, _needed)]),
    );
    final focus = _at(tester, 1);

    await tester.pumpWidget(_chainHero([(1, _done), (2, _done), (3, _needed)]));
    await _pan(tester);

    expect(_at(tester, 3), offsetMoreOrLessEquals(focus));

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a device plugged in mid-check keeps its place, gets no check, '
      'and the camera skips it', (tester) async {
    await tester.pumpWidget(
      _chainHero([(1, _needed), (4, _stranger), (3, _needed)]),
    );
    final focus = _at(tester, 1);
    final step = _at(tester, 4) - focus;

    expect(_checked(4), isFalse);

    await tester.pumpWidget(
      _chainHero([(1, _done), (4, _stranger), (3, _needed)]),
    );
    await _pan(tester);

    expect(_at(tester, 3), offsetMoreOrLessEquals(focus));
    expect(_at(tester, 4), offsetMoreOrLessEquals(focus - step));
    expect(_checked(4), isFalse);
    expect(_checked(1), isTrue);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('plugging a device in ahead of the focus reframes at once and '
      'keeps its demo playing', (tester) async {
    await tester.pumpWidget(
      _chainHero([(1, _done), (2, _needed), (3, _needed)]),
    );
    await _pan(tester);
    final focus = _at(tester, 2);
    final playing = _playback(tester);

    await tester.pumpWidget(
      _chainHero([(1, _done), (4, _stranger), (2, _needed), (3, _needed)]),
    );
    await tester.pump();

    expect(_at(tester, 2), offsetMoreOrLessEquals(focus));
    expect(_playback(tester), same(playing));

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'unplugging the focus device starts the demo afresh on the next',
    (tester) async {
      await tester.pumpWidget(
        _chainHero([(1, _done), (2, _needed), (3, _needed)]),
      );
      await _pan(tester);
      final focus = _at(tester, 2);
      final playing = _playback(tester);

      await tester.pumpWidget(_chainHero([(1, _done), (3, _needed)]));
      await tester.pump();

      expect(_at(tester, 3), offsetMoreOrLessEquals(focus));
      expect(_playback(tester), isNot(same(playing)));

      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('the last confirmation stays on the hero, where the camera was, '
      'without the hand', (tester) async {
    await tester.pumpWidget(_chainHero([(1, _done), (2, _needed)]));
    await _pan(tester);
    final focus = _at(tester, 2);
    expect(_hand, findsOneWidget);

    await tester.pumpWidget(_chainHero([(1, _done), (2, _done)]));
    await _pan(tester);

    expect(find.byType(DeviceActionHero), findsOneWidget);
    expect(_at(tester, 2), offsetMoreOrLessEquals(focus));
    expect(_checked(2), isTrue);
    expect(_hand, findsNothing);

    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('unplugging the focus device once the rest are done leaves the '
      'hero up, without the hand', (tester) async {
    await tester.pumpWidget(_chainHero([(1, _done), (2, _needed)]));
    await _pan(tester);

    await tester.pumpWidget(_chainHero([(1, _done)]));
    await tester.pump();

    expect(tester.takeException(), isNull);
    expect(_checked(1), isTrue);
    expect(_hand, findsNothing);

    await tester.pumpWidget(const SizedBox());
  });

  test('a chain hero has nothing to draw once no participant is connected', () {
    expect(
      DeviceActionHero.of(_keygenSpec, _chainOf([(4, _stranger)])),
      isNull,
    );
    expect(DeviceActionHero.of(_keygenSpec, []), isNull);
  });

  test('a single-device hero draws only the first device still needed', () {
    final spec = DeviceActionHeroSpec(demo: () => const DeviceDemoKind.sign());
    final hero = DeviceActionHero.of(
      spec,
      _chainOf([(1, _done), (2, _needed), (3, _needed)]),
    );
    expect(hero?.chain.map((device) => device.id), [_id(2)]);
    expect(DeviceActionHero.of(spec, _chainOf([(1, _done)])), isNull);
  });

  test('a device is done only if it took part and has left the action', () {
    DeviceId id(int n) => DeviceId(field0: U8Array33(Uint8List(33)..[0] = n));
    final participants = {id(1), id(3)};
    final actionNeeded = {id(3)};
    ChainRole role(int n) => FullscreenActionDialogController.chainRole(
      id(n),
      participants: participants,
      actionNeeded: actionNeeded,
    );

    expect(role(1), ChainRole.done);
    expect(role(2), ChainRole.notTakingPart);
    expect(role(3), ChainRole.needed);
  });
}
