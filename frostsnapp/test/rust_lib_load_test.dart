import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/name.dart';

import 'support/rust_lib.dart';

void main() {
  setUpAll(initRustLibForTests);

  test('Rust library loads and answers a call', () {
    expect(keyNameMaxLength(), greaterThan(0));
  });
}
