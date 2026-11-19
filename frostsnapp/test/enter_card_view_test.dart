import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/restoration/enter_card_view.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:google_fonts/google_fonts.dart';

// On phones the recovery flow draws no top bar, so the card's entry view
// carries its own title there.

class _Driver implements CardRestoreDriver {
  @override
  Stream<EnterPhysicalBackupState> start() => const Stream.empty();
  @override
  Future<void> sendPhoneHalf(List<String?> words) async {}
  @override
  Stream<bool> deviceConnected() => Stream.value(true);
}

Future<void> _pumpAt(WidgetTester tester, Size size) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: EnterCardView(
          driver: _Driver(),
          lookup: WordLookup(isWord: (_) => true),
          onFinished: (_) {},
          onCancel: () {},
          onError: (_) {},
        ),
      ),
    ),
  );
}

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  testWidgets('the card entry shows its title on a phone', (tester) async {
    await _pumpAt(tester, const Size(400, 800));
    expect(find.text('Enter your card'), findsOne);
  });

  testWidgets('the card entry leaves the title to the top bar on a tablet', (
    tester,
  ) async {
    await _pumpAt(tester, const Size(900, 800));
    expect(find.text('Enter your card'), findsNothing);
  });
}
