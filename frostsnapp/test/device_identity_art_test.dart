import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';
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
const _oldUnattested = GenuineStatus.unattested(firmwareSupportsCheck: false);
const _oldAttested = GenuineStatus.attested(firmwareSupportsCheck: false);

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

String _silhouetteShown(WidgetTester tester) {
  final picture = tester.widget<SvgPicture>(find.byType(SvgPicture));
  return switch (picture.bytesLoader) {
    SvgAssetLoader(:final assetName) => assetName,
    final other => fail('unexpected loader $other'),
  };
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

Finder _badge(IconData icon) => find.descendant(
  of: find.byType(DeviceIdentityArt),
  matching: find.byIcon(icon),
);

void main() {
  setUpAll(() async {
    GoogleFonts.config.allowRuntimeFetching = false;
    genuineCheckEnabled = true;
    await initRustLibForTests();
  });

  group('DeviceIdentityArt', () {
    testWidgets('a known case colour draws that colour\'s silhouette', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          const DeviceIdentityArt(
            caseColor: CaseColor.red(),
            genuine: _unattested,
          ),
        ),
      );
      expect(_silhouetteShown(tester), endsWith('frostsnap-device-red.svg'));
    });

    for (final (what, caseColor) in [
      ('no colour on file', null),
      ('a colour this build has no name for', const CaseColor.other(9)),
    ]) {
      testWidgets('$what draws the unknown-colour outline', (tester) async {
        await tester.pumpWidget(
          _app(DeviceIdentityArt(caseColor: caseColor, genuine: _unattested)),
        );
        expect(
          _silhouetteShown(tester),
          endsWith('frostsnap-device-unknown.svg'),
        );
        expect(
          _artSemantics(tester),
          matchesSemantics(
            label:
                'Case colour unknown, Unverified. Show genuine-check details',
            isButton: true,
            hasTapAction: true,
            hasFocusAction: true,
            isFocusable: true,
          ),
        );
      });
    }

    for (final (what, status, icon, label) in [
      (
        'genuine',
        _genuine(const CaseColor.red()),
        Icons.verified_rounded,
        'Genuine',
      ),
      ('attested', _attested, Icons.gpp_maybe_rounded, 'Unverified'),
      ('unattested', _unattested, Icons.gpp_maybe_rounded, 'Unverified'),
      (
        'attested, old firmware',
        _oldAttested,
        Icons.system_update_rounded,
        'Update to verify',
      ),
      (
        'unattested, old firmware',
        _oldUnattested,
        Icons.system_update_rounded,
        'Update to verify',
      ),
    ]) {
      testWidgets('$what is badged "$label" with no spinner', (tester) async {
        await tester.pumpWidget(
          _app(
            DeviceIdentityArt(
              caseColor: const CaseColor.red(),
              genuine: status,
            ),
          ),
        );
        expect(_badge(icon), findsOneWidget);
        expect(find.byType(CircularProgressIndicator), findsNothing);
        expect(
          _artSemantics(tester).label,
          'Red case, $label. Show genuine-check details',
        );
      });
    }

    testWidgets('only a genuine device is badged in the primary colour', (
      tester,
    ) async {
      Future<Color?> badgeColour(GenuineStatus status, IconData icon) async {
        await tester.pumpWidget(
          _app(
            DeviceIdentityArt(
              caseColor: const CaseColor.red(),
              genuine: status,
            ),
          ),
        );
        return tester.widget<Icon>(_badge(icon)).color;
      }

      final genuineColour = await badgeColour(
        _genuine(const CaseColor.red()),
        Icons.verified_rounded,
      );
      final primary = Theme.of(
        tester.element(find.byType(Scaffold)),
      ).colorScheme.primary;
      expect(genuineColour, primary);
      expect(
        await badgeColour(_attested, Icons.gpp_maybe_rounded),
        isNot(primary),
      );
    });

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

    for (final (how, close) in <(String, Future<void> Function(WidgetTester))>[
      ('Got it', (t) => t.tap(find.text('Got it'))),
      ('Escape', (t) => t.sendKeyEvent(LogicalKeyboardKey.escape)),
    ]) {
      testWidgets('keyboard opens the details and focus returns after $how', (
        tester,
      ) async {
        await tester.pumpWidget(
          _app(
            const DeviceIdentityArt(
              caseColor: CaseColor.silver(),
              genuine: _unattested,
            ),
          ),
        );
        bool artFocused() =>
            FocusManager.instance.primaryFocus?.context
                ?.findAncestorWidgetOfExactType<DeviceIdentityArt>() !=
            null;

        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pump();
        expect(artFocused(), isTrue);

        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pumpAndSettle();
        expect(find.text('Not verified'), findsOneWidget);

        await close(tester);
        await tester.pumpAndSettle();
        expect(find.text('Not verified'), findsNothing);
        expect(artFocused(), isTrue);
      });
    }
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

    testWidgets('a genuine red device has no warning and no status pill', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          row(
            _device(
              caseColor: const CaseColor.red(),
              genuine: _genuine(const CaseColor.red()),
            ),
            controller: TextEditingController(text: 'Lawyer'),
          ),
        ),
      );
      expect(find.byType(GenuineBadge), findsNothing);
      expect(find.text('Genuine'), findsNothing);
      expect(find.byType(DeviceNotice), findsNothing);
      expect(find.byIcon(Icons.warning_rounded), findsNothing);
      expect(_badge(Icons.verified_rounded), findsOneWidget);
    });

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

    testWidgets('the art is still a button that opens the details', (
      tester,
    ) async {
      await tester.pumpWidget(_app(row(_device())));
      expect(
        _artSemantics(tester).getSemanticsData().flagsCollection.isButton,
        isTrue,
      );

      await tester.tap(find.byType(DeviceIdentityArt));
      await tester.pumpAndSettle();
      expect(find.text('Not verified'), findsOneWidget);
    });

    testWidgets('an unnamed device says it needs a name until it has one', (
      tester,
    ) async {
      final controller = TextEditingController();
      await tester.pumpWidget(_app(row(_device(), controller: controller)));
      expect(find.text('Name it to continue'), findsOneWidget);

      await tester.enterText(find.byType(TextField), 'Office');
      await tester.pump();
      expect(find.text('Name it to continue'), findsNothing);
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
