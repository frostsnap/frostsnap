import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/name.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/verifiable_card/card_flow_preview.dart';

import 'support/card_flow_pages.dart';
import 'support/rust_lib.dart';

void main() {
  setUpAll(initRustLibForTests);

  test('Rust library loads and answers a call', () {
    expect(keyNameMaxLength(), greaterThan(0));
  });

  test("the fakes' seal is the card's", () {
    // The flow tests and the debug preview fake the bars' total; the app
    // takes it from the bridge.
    expect(cardSealSize(), 311);
    expect(fakeSealSize, cardSealSize());
    expect(previewSealSize, cardSealSize());
  });
}
