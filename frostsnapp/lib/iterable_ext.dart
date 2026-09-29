extension SortedByCachedKey<T> on Iterable<T> {
  /// Sorts by `key`, calling it once per item, for keys too costly to recompute on every
  /// comparison. Items with equal keys keep their original order.
  List<T> sortedByCachedKey<K extends Comparable<K>>(K Function(T) key) {
    final keyed = [
      for (final (i, item) in indexed) (key: key(item), index: i, item: item),
    ];
    // List.sort isn't guaranteed stable.
    keyed.sort((a, b) {
      final byKey = a.key.compareTo(b.key);
      return byKey != 0 ? byKey : a.index.compareTo(b.index);
    });
    return [for (final entry in keyed) entry.item];
  }
}
