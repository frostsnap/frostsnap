import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_hero.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

import 'support/rust_lib.dart';

const _hero = DeviceActionHero(
  caseColor: CaseColor_Orange(),
  demo: DeviceDemoKind.sign,
  maxHeight: 430,
);

void main() {
  setUpAll(initRustLibForTests);

  testWidgets('the hand keeps animating while no device frames arrive', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(home: SizedBox(width: 400, child: _hero)),
    );
    for (var i = 0; i < 10; i++) {
      expect(tester.binding.hasScheduledFrame, isTrue);
      await tester.pump(const Duration(milliseconds: 33));
    }

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
}
