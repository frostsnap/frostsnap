import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/wallet_contacts.dart';

const address = 'tsp1qabcdef';

void main() {
  group('contactNameError', () {
    test('a new name is fine, and an empty one has nothing to report', () {
      expect(contactNameError('Bob', ['Alice']), isNull);
      expect(contactNameError('  ', ['Alice']), isNull);
    });

    test('a name in use is refused whatever its case or padding', () {
      expect(contactNameError('Alice', ['Alice']), isNotNull);
      expect(contactNameError('  aLICE ', ['Alice']), isNotNull);
    });

    test('a name the devices would refuse is reported', () {
      expect(contactNameError('Alіce', []), isNotNull);
    });
  });

  group('contactErrorMessage', () {
    test('drops the backtrace of a Rust error', () {
      final error = AnyhowException(
        'Bad address\n\nStack backtrace:\n   0: <std::backtrace::Backtrace>::create',
      );
      expect(contactErrorMessage(error), 'Bad address');
    });

    test('keeps a Rust error with no backtrace', () {
      expect(
        contactErrorMessage(AnyhowException('Bad address')),
        'Bad address',
      );
    });
  });

  group('contactMatches', () {
    bool matches(String query) =>
        contactMatches(query, name: 'Alice Payroll', address: address);

    test('finds part of a name or address, ignoring case', () {
      expect(matches('pay'), isTrue);
      expect(matches('ABCD'), isTrue);
      expect(matches(''), isTrue);
      expect(matches('bob'), isFalse);
    });
  });

  group('contactIsExactly', () {
    bool exact(String query) =>
        contactIsExactly(query, name: 'Alice', address: address);

    test('only the whole name or address selects a contact', () {
      expect(exact(' alice '), isTrue);
      expect(exact(address.toUpperCase()), isTrue);
      expect(exact('Ali'), isFalse);
      expect(exact('tsp1q'), isFalse);
    });
  });
}
