import 'dart:async';
import 'package:rxdart/rxdart.dart';

extension StreamToBehaviorSubjectExtension<T> on Stream<T> {
  BehaviorSubject<T> toBehaviorSubject({T? seedValue}) {
    // Initialize the BehaviorSubject with a seed value if provided
    final BehaviorSubject<T> subject = seedValue != null
        ? BehaviorSubject.seeded(seedValue)
        : BehaviorSubject<T>();

    listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () => subject.close(),
    );

    return subject;
  }
}

extension StreamCompletionFuture<T> on Stream<T> {
  Future<void> get completionFuture {
    final Completer<void> completer = Completer<void>();

    this.listen(
      (event) {
        // Do nothing with the events
      },
      onDone: () {
        completer.complete();
      },
      onError: (error) {
        completer.completeError(error);
      },
      cancelOnError: true,
    );

    return completer.future;
  }
}
