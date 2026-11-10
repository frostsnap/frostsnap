import 'dart:async';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_hero.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

import 'support/rust_lib.dart';

const _hero = DeviceActionHero(
  caseColor: CaseColor_Orange(),
  demo: DeviceDemoKind.sign(),
  maxHeight: 430,
);

void main() {
  setUpAll(initRustLibForTests);

  testWidgets('a hero with nothing to play leaves the engine idle', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: SizedBox(
          width: 400,
          child: DeviceActionHero(
            caseColor: CaseColor_Orange(),
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
}
