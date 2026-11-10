import 'dart:async';

import 'package:dotted_border/dotted_border.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/access_structures.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/device_identity_art.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_create.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:rxdart/rxdart.dart';

import 'support/rust_lib.dart';

const _unverified = GenuineStatus.unverified(firmwareSupportsCheck: true);

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
  GenuineStatus genuine = _unverified,
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
    genuine: genuine,
  );
}

Widget _app(Widget child, {double width = 360, TextScaler? textScaler}) {
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
          child: SingleChildScrollView(
            child: textScaler == null
                ? child
                : Builder(
                    builder: (context) => MediaQuery(
                      data: MediaQuery.of(
                        context,
                      ).copyWith(textScaler: textScaler),
                      child: child,
                    ),
                  ),
          ),
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
          DeviceIdentityArt.forDevice(
            _device(genuine: _genuine(const CaseColor.red())),
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

    testWidgets('an unverified device shows no colour and no certificate', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(DeviceIdentityArt.forDevice(_device(genuine: _unverified))),
      );
      expect(
        _artSemantics(tester).label,
        startsWith('Case colour unknown, Unverified'),
      );
      await tester.tap(find.byType(DeviceIdentityArt));
      await tester.pumpAndSettle();

      expect(find.text('Unverified device'), findsOneWidget);
      expect(find.text('Serial'), findsNothing);
    });
  });

  group('CaseAccent', () {
    testWidgets(
      'a device pictured beside its name, connected and listed or not, shows a colour only '
      'once it proves itself, and the colour on file only once it leaves',
      (tester) async {
        final unverified = _device(genuine: _unverified);
        final genuine = _device(genuine: _genuine(const CaseColor.blue()));
        final devices = StreamController<DeviceListUpdate>(sync: true);
        addTearDown(devices.close);
        caseAccentSource = (
          devices: devices.stream,
          recorded: (_) => const CaseColor.red(),
        );
        DeviceListUpdate connected({
          List<ConnectedDevice> listed = const [],
          List<ConnectedDevice> unlisted = const [],
        }) => DeviceListUpdate(
          changes: const [],
          state: DeviceListState(
            devices: listed,
            unlisted: unlisted,
            stateId: 0,
          ),
        );
        CaseColor? shown() => tester
            .widget<DeviceNameWithArt>(find.byType(DeviceNameWithArt))
            .caseColor;

        await tester.pumpWidget(
          _app(DeviceNameWithArt.forId(unverified.id, name: Text('Office'))),
        );
        expect(shown(), isNull);

        devices.add(connected(unlisted: [unverified]));
        await tester.pump();
        expect(shown(), isNull);

        devices.add(connected(listed: [unverified]));
        await tester.pump();
        expect(shown(), isNull);
        expect(find.byType(DottedBorder), findsOneWidget);

        devices.add(connected(listed: [genuine]));
        await tester.pump();
        expect(shown(), const CaseColor.blue());

        devices.add(connected());
        await tester.pump();
        expect(shown(), const CaseColor.red());
      },
    );
  });

  group('DeviceNameWithArt', () {
    testWidgets('reads as the name alone and carries no genuine badge', (
      tester,
    ) async {
      final semantics = tester.ensureSemantics();
      await tester.pumpWidget(
        _app(
          const DeviceNameWithArt(
            caseColor: CaseColor.orange(),
            name: Text('Kitchen'),
          ),
        ),
      );
      expect(find.byType(Icon), findsNothing);
      expect(find.byType(InkWell), findsNothing);
      expect(find.byType(DottedBorder), findsNothing);
      expect(find.bySemanticsLabel(RegExp('case')), findsNothing);
      expect(find.bySemanticsLabel('Kitchen'), findsOneWidget);
      semantics.dispose();
    });

    for (final (what, caseColor) in [
      ('no colour on file', null),
      ('a colour this build has no name for', const CaseColor.other(9)),
    ]) {
      testWidgets('$what draws the dashed outline', (tester) async {
        await tester.pumpWidget(
          _app(DeviceNameWithArt(caseColor: caseColor, name: Text('Office'))),
        );
        expect(find.byType(DottedBorder), findsOneWidget);
      });
    }

    testWidgets('a long name ellipsizes beside the art at phone width', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          const DeviceNameWithArt(
            caseColor: CaseColor.blue(),
            name: Text(
              'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
              overflow: TextOverflow.ellipsis,
            ),
          ),
          width: 200,
        ),
      );
      expect(tester.takeException(), isNull);
      final row = tester.getRect(find.byType(DeviceNameWithArt));
      final name = tester.getRect(find.byType(Text));
      expect(name.right, lessThanOrEqualTo(row.right));
      expect(name.left, greaterThan(row.left + DeviceNameWithArt.artSize));
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
                  _device(genuine: _genuine(const CaseColor.blue())),
                  controller: TextEditingController(text: 'WWWWWWWWWWWWWW'),
                ),
                row(
                  _device(
                    seed: 2,
                    name: 'WWWWWWWWWWWWWW',
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

  group('DeviceTile', () {
    setUp(() {
      caseAccentSource = (
        devices: BehaviorSubject.seeded(
          DeviceListUpdate(
            changes: const [],
            state: DeviceListState(
              devices: const [],
              unlisted: const [],
              stateId: 0,
            ),
          ),
        ),
        recorded: (_) => const CaseColor.orange(),
      );
    });

    Widget grid(List<String> names, {int firstIndex = 1}) => DeviceTileGrid(
      tiles: [
        for (final (i, name) in names.indexed)
          DeviceTile(
            id: _device(seed: i + 1).id,
            name: name,
            shareIndex: firstIndex + i,
            corner: const Icon(Icons.radio_button_unchecked, size: 18),
          ),
      ],
    );

    for (final (width, scale) in [(268.0, 1.0), (268.0, 2.0), (528.0, 1.3)]) {
      testWidgets(
        'the key index never covers the USB slot ($width wide, text x$scale)',
        (tester) async {
          tester.view.devicePixelRatio = 1;
          tester.view.physicalSize = const Size(1200, 2400);
          addTearDown(tester.view.reset);
          await tester.pumpWidget(
            _app(
              grid([
                'Daily carry',
                'Safe deposit 1',
                'Spare drawer',
              ], firstIndex: 98),
              width: width,
              textScaler: TextScaler.linear(scale),
            ),
          );
          await tester.pumpAndSettle();

          final tiles = find.byType(DeviceTile);
          expect(tiles, findsNWidgets(3));
          expect(tester.getSize(find.byType(DeviceTileGrid)).width, width);
          expect(
            MediaQuery.textScalerOf(tester.element(tiles.first)).scale(10),
            10 * scale,
          );
          for (var i = 0; i < 3; i++) {
            final tile = tiles.at(i);
            final art = tester.getRect(
              find.descendant(of: tile, matching: find.byType(SvgPicture)),
            );
            // The slot is drawn x 32-48, y 5-19 of the 80x96 viewBox, with a 2-unit stroke.
            final slot = Rect.fromLTRB(
              art.left + art.width * 31 / 80,
              art.top + art.height * 4 / 96,
              art.left + art.width * 49 / 80,
              art.top + art.height * 20 / 96,
            );
            final tileRect = tester.getRect(tile);
            expect(art.center.dx, moreOrLessEquals(tileRect.center.dx));
            final index = tester.getRect(
              find.descendant(of: tile, matching: find.text('#${98 + i}')),
            );
            expect(
              index.right,
              lessThanOrEqualTo(slot.left),
              reason: 'index $index reaches into the slot\'s column $slot',
            );
            expect(
              index.overlaps(slot),
              isFalse,
              reason: 'index $index overlaps slot $slot',
            );
          }
        },
      );
    }

    testWidgets(
      'a full-length name wraps without clipping and its row stays level',
      (tester) async {
        await tester.pumpWidget(
          _app(
            grid(['Kitchen', 'ABCDEFGHIJKLMN', 'Safe deposit 1', 'Hall']),
            width: 268,
          ),
        );
        await tester.pumpAndSettle();

        final heights = [
          for (var i = 0; i < 3; i++)
            tester.getSize(find.byType(DeviceTile).at(i)).height,
        ];
        expect(heights.toSet(), hasLength(1));

        final long = tester.renderObject<RenderParagraph>(
          find.text('ABCDEFGHIJKLMN'),
        );
        expect(long.didExceedMaxLines, isFalse);
        expect(long.size.height, greaterThan(16));
      },
    );

    testWidgets('a corner control is its own named button', (tester) async {
      var removed = 0;
      await tester.pumpWidget(
        _app(
          DeviceTileGrid(
            tiles: [
              DeviceTile(
                id: _device().id,
                name: 'Office',
                shareIndex: 1,
                corner: const Icon(Icons.close, size: 18),
                cornerTooltip: 'Remove key',
                onCornerTap: () => removed++,
              ),
            ],
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        tester.getSemantics(find.byTooltip('Remove key')),
        matchesSemantics(
          tooltip: 'Remove key',
          isButton: true,
          hasTapAction: true,
          isFocusable: true,
          hasFocusAction: true,
        ),
      );
      await tester.tap(find.byTooltip('Remove key'));
      expect(removed, 1);
    });
  });

  group('KeySetSummary', () {
    testWidgets('devices holding copies of one key count as one key', (
      tester,
    ) async {
      await tester.pumpWidget(
        _app(
          const KeySetSummary(
            threshold: 2,
            shareIndices: [1, 1, 2],
            walletName: 'Office',
          ),
        ),
      );
      expect(find.textContaining('of these 2 keys can spend'), findsOneWidget);
      expect(find.text('0 spare'), findsOneWidget);
      expect(
        find.text('Office needs every one of these keys to spend.'),
        findsOneWidget,
      );
      expect(
        find.text('Devices showing the same number hold copies of one key.'),
        findsOneWidget,
      );
    });

    testWidgets('distinct keys beyond the threshold are spare', (tester) async {
      await tester.pumpWidget(
        _app(
          const KeySetSummary(
            threshold: 3,
            shareIndices: [1, 2, 3, 4, 5],
            walletName: 'Office',
          ),
        ),
      );
      expect(find.textContaining('of these 5 keys can spend'), findsOneWidget);
      expect(
        find.text('Office can still be spent if any 2 keys are lost.'),
        findsOneWidget,
      );
      expect(find.textContaining('copies of one key'), findsNothing);
    });
  });
}
