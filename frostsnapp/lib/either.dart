class Either<L, R> {
  final L? _left;
  final R? _right;

  Either.left(this._left) : _right = null;
  Either.right(this._right) : _left = null;

  T match<T>({
    required T Function(L) left,
    required T Function(R) right,
  }) {
    if (_left != null) {
      return left(_left as L);
    } else {
      return right(_right as R);
    }
  }

  bool get isLeft => _left != null;
  bool get isRight => _right != null;
}
