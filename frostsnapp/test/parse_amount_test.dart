import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/transaction.dart';

import 'support/rust_lib.dart';

void main() {
  setUpAll(initRustLibForTests);

  test('the largest amount crosses the bridge unchanged', () {
    expect(
      parseAmount(text: '21000000', denomination: AmountDenomination.bitcoin),
      2100000000000000,
    );
  });

  test('amounts beyond i64 are rejected, not saturated', () {
    for (final (text, denomination) in [
      ('9223372036854775808', AmountDenomination.satoshi),
      ('18446744073709551615', AmountDenomination.satoshi),
      ('92233720368.54775808', AmountDenomination.bitcoin),
    ]) {
      expect(
        () => parseAmount(text: text, denomination: denomination),
        throwsA(ParseAmountError.tooLarge),
        reason: text,
      );
    }
  });
}
