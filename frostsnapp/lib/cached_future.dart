class CachedFuture<T> {
  Future<T> future;
  T? result;

  CachedFuture(this.future);

  Future<T> get value async {
    result ??= await future;
    return result!;
  }
}
