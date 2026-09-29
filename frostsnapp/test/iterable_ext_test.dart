import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/iterable_ext.dart';

void main() {
  test('computes each key exactly once', () {
    final calls = <int, int>{};
    final sorted = [5, 3, 9, 1, 7, 2, 8].sortedByCachedKey((n) {
      calls[n] = (calls[n] ?? 0) + 1;
      return n;
    });
    expect(sorted, [1, 2, 3, 5, 7, 8, 9]);
    expect(calls.values, everyElement(1));
    expect(calls.length, 7);
  });

  test('equal keys keep their original order', () {
    // Long enough that List.sort leaves insertion sort, which would hide instability.
    final items = [for (var i = 0; i < 200; i++) (key: i % 3, position: i)];
    final sorted = items.sortedByCachedKey((item) => item.key);
    for (var key = 0; key < 3; key++) {
      final positions = [
        for (final item in sorted)
          if (item.key == key) item.position,
      ];
      expect(positions, [for (var i = key; i < 200; i += 3) i]);
    }
  });
}
