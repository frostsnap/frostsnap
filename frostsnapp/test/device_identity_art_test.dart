import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_identity_art.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_create.dart';
import 'package:google_fonts/google_fonts.dart';

import 'support/rust_lib.dart';

const _unattested = GenuineStatus.unattested(firmwareSupportsCheck: true);
const _attested = GenuineStatus.attested(firmwareSupportsCheck: true);

GenuineStatus _genuine(CaseColor caseColor) => GenuineStatus.genuine(
  certificate: GenuineCertificate(
    serial: '220825002',
    revision: '2.7-1625',
    caseColor: caseColor,
    provisionedAt: 1773801360,
    dsPublicKey: Uint8List(0),
  ),
);

ConnectedDevice _device({
  int seed = 1,
  String? name,
  CaseColor? caseColor,
  GenuineStatus genuine = _unattested,
}) {
  final id = Uint8List(33);
  id[0] = seed;
  return ConnectedDevice(
    name: name,
    firmware: FirmwareVersion(
      digest: Sha256Digest(field0: U8Array32(Uint8List(32))),
    ),
    id: DeviceId(field0: U8Array33(id)),
    recoveryMode: const RecoveryMode.off(),
    caseColor: caseColor,
    genuine: genuine,
  );
}

Widget _app(Widget child, {double width = 360}) {
  final scheme = ColorScheme.fromSeed(
    brightness: Brightness.dark,
    seedColor: seedColor,
  );
  return MaterialApp(
    theme: ThemeData(useMaterial3: true, colorScheme: scheme),
    home: Scaffold(
      body: Align(
        alignment: Alignment.topCenter,
        child: SizedBox(
          width: width,
          child: SingleChildScrollView(child: child),
        ),
      ),
    ),
  );
}

SemanticsNode _artSemantics(WidgetTester tester) => tester.getSemantics(
  find
      .descendant(
        of: find.byType(DeviceIdentityArt),
        matching: find.byWidgetPredicate(
          (w) => w is Semantics && w.properties.label != null,
        ),
      )
      .first,
);

bool _withinTextField(SemanticsNode node) {
  for (SemanticsNode? n = node; n != null; n = n.parent) {
    if (n.getSemanticsData().flagsCollection.isTextField) return true;
  }
  return false;
}

void main() {
  setUpAll(() async {
    GoogleFonts.config.allowRuntimeFetching = false;
    genuineCheckEnabled = true;
    await initRustLibForTests();
  });

  group('DeviceIdentityArt', () {
    testWidgets('a build that cannot check shows no badge and nothing to tap', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          const DeviceIdentityArt(caseColor: CaseColor.blue(), genuine: null),
        ),
      );
      expect(
        find.descendant(
          of: find.byType(DeviceIdentityArt),
          matching: find.byType(Icon),
        ),
        findsNothing,
      );
      expect(
        find.descendant(
          of: find.byType(DeviceIdentityArt),
          matching: find.byType(InkWell),
        ),
        findsNothing,
      );
      expect(
        _artSemantics(tester),
        matchesSemantics(label: 'Blue case', isImage: true),
      );
    });

    testWidgets('tapping a genuine device shows its factory certificate', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          DeviceIdentityArt(
            caseColor: const CaseColor.red(),
            genuine: _genuine(const CaseColor.red()),
          ),
        ),
      );
      await tester.tap(find.byType(DeviceIdentityArt));
      await tester.pumpAndSettle();

      expect(find.text('Genuine device'), findsOneWidget);
      expect(find.text('220825002'), findsOneWidget);
      expect(find.text('2.7-1625'), findsOneWidget);
      expect(find.text('Red'), findsOneWidget);
    });

    testWidgets('an attested device is not shown as genuine and shows no '
        'certificate', (tester) async {
      await tester.pumpWidget(
        _app(
          const DeviceIdentityArt(
            caseColor: CaseColor.red(),
            genuine: _attested,
          ),
        ),
      );
      await tester.tap(find.byType(DeviceIdentityArt));
      await tester.pumpAndSettle();

      expect(find.text('Not verified'), findsOneWidget);
      expect(find.text('Serial'), findsNothing);
    });
  });

  group('AddDeviceRow', () {
    Widget row(
      ConnectedDevice device, {
      FirmwareUpgradeEligibility eligibility =
          const FirmwareUpgradeEligibility.upToDate(),
      TextEditingController? controller,
    }) => AddDeviceRow(
      device: device,
      eligibility: eligibility,
      nameEditor: (_) => DeviceNameField(
        controller: controller ?? TextEditingController(),
        focusNode: FocusNode(),
        isDuplicate: false,
        onChanged: (_) {},
      ),
      onEditName: () {},
      onUpgrade: () {},
    );

    testWidgets('a device that already holds a key says to unplug it', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          row(
            _device(
              name: 'SEMIIMPORTANT',
              caseColor: const CaseColor.silver(),
              genuine: _genuine(const CaseColor.silver()),
            ),
          ),
        ),
      );
      expect(find.text('SEMIIMPORTANT'), findsOneWidget);
      expect(find.text('Already holds a key'), findsOneWidget);
      expect(find.text('Unplug to continue'), findsOneWidget);
      expect(find.byType(TextField), findsNothing);
    });

    testWidgets('firmware problems keep their guidance', (tester) async {
      await tester.pumpWidget(
        _app(
          Column(
            children: [
              row(
                _device(),
                eligibility: const FirmwareUpgradeEligibility.canUpgrade(),
              ),
              row(
                _device(seed: 2),
                eligibility: const FirmwareUpgradeEligibility.cannotUpgrade(
                  reason: 'Device firmware is newer than this app',
                ),
              ),
            ],
          ),
        ),
      );
      expect(find.text('Old firmware'), findsOneWidget);
      expect(find.text('Tap to upgrade'), findsOneWidget);
      expect(find.text('Incompatible firmware'), findsOneWidget);
      expect(
        find.text('Device firmware is newer than this app'),
        findsOneWidget,
      );
      expect(find.byType(TextField), findsNothing);
    });

    testWidgets('Tab moves only between the name fields, in row order', (
      tester,
    ) async {
      final controllers = List.generate(3, (_) => TextEditingController());
      await tester.pumpWidget(
        _app(
          Column(
            children: [
              for (final (i, controller) in controllers.indexed)
                row(_device(seed: i + 1), controller: controller),
            ],
          ),
        ),
      );

      TextEditingController? focusedField() => FocusManager
          .instance
          .primaryFocus
          ?.context
          ?.findAncestorWidgetOfExactType<TextField>()
          ?.controller;

      final visited = <TextEditingController?>[];
      for (var i = 0; i < 6; i++) {
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pump();
        visited.add(focusedField());
      }
      expect(visited, [...controllers, ...controllers]);
    });

    for (final (layout, width) in [('phone', 358.0), ('desktop', 600.0)]) {
      testWidgets('at $layout width nothing overlaps the name', (tester) async {
        await tester.pumpWidget(
          _app(
            Column(
              children: [
                row(
                  _device(caseColor: const CaseColor.blue()),
                  controller: TextEditingController(text: 'WWWWWWWWWWWWWW'),
                ),
                row(
                  _device(
                    seed: 2,
                    name: 'WWWWWWWWWWWWWW',
                    caseColor: const CaseColor.silver(),
                    genuine: _genuine(const CaseColor.silver()),
                  ),
                ),
              ],
            ),
            width: width,
          ),
        );
        expect(tester.takeException(), isNull);

        final cards = find.byType(DeviceArtCard);
        for (var i = 0; i < 2; i++) {
          final card = cards.at(i);
          final art = tester.getRect(
            find.descendant(of: card, matching: find.byType(DeviceIdentityArt)),
          );
          final name = tester.getRect(
            find
                .descendant(
                  of: card,
                  matching: find.textContaining('WWWWWWWWWWWWWW'),
                )
                .first,
          );
          expect(
            art.right,
            lessThan(name.left),
            reason: 'badge or art overlaps the name ($layout, row $i)',
          );
          expect(art.overlaps(name), isFalse);
        }

        final notice = tester.getRect(find.byType(DeviceNotice));
        final holderName = tester.getRect(
          find.descendant(
            of: cards.at(1),
            matching: find.text('WWWWWWWWWWWWWW'),
          ),
        );
        expect(notice.overlaps(holderName), isFalse);
        if (width >= DeviceArtCard.sideNoticeMinWidth + 28) {
          expect(notice.left, greaterThan(holderName.right));
        } else {
          expect(notice.top, greaterThan(holderName.bottom));
        }
      });
    }
  });

  group('DeviceNameField', () {
    Widget field({required String text, required bool isDuplicate}) =>
        DeviceNameField(
          controller: TextEditingController(text: text),
          focusNode: FocusNode(),
          isDuplicate: isDuplicate,
          onChanged: (_) {},
        );

    for (final (what, text, isDuplicate, message) in [
      ('a duplicate name', 'Office', true, 'Name already used in this wallet'),
      ('a missing name', '', false, 'Name it to continue'),
    ]) {
      testWidgets('$what is announced as part of the name field', (
        tester,
      ) async {
        await tester.pumpWidget(
          _app(field(text: text, isDuplicate: isDuplicate)),
        );
        expect(
          _withinTextField(tester.getSemantics(find.text(message))),
          isTrue,
          reason: 'a screen reader editing the name would not hear "$message"',
        );
      });
    }
  });
}
