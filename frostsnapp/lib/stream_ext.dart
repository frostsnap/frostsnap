import 'dart:async';
import 'package:rxdart/rxdart.dart';

extension StreamToBehaviorSubjectExtension<T> on Stream<T> {
  /// Converts the current [Stream<T>] into a [BehaviorSubject<T>].
  ///
  /// [seedValue] is an optional initial value that the BehaviorSubject holds.
  BehaviorSubject<T> toBehaviorSubject({T? seedValue}) {
    // Initialize the BehaviorSubject with a seed value if provided
    final BehaviorSubject<T> subject = seedValue != null
        ? BehaviorSubject.seeded(seedValue)
        : BehaviorSubject<T>();

    // Listen to the original stream and forward events to the BehaviorSubject
    // CRITICAL FIX: Store subscription so it can be cancelled
    late StreamSubscription<T> subscription;
    subscription = listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () {
        subject.close();
        // No need to cancel subscription here as stream is done
      },
    );

    // CRITICAL FIX: Cancel subscription when subject is closed manually
    // This prevents memory leaks when the subject is disposed before stream completes
    subject.onCancel = () {
      subscription.cancel();
    };

    return subject;
  }

  /// Converts the current [Stream<T>] into a [ReplaySubject<T>].
  ///
  /// [bufferSize] determines how many past events to replay to new subscribers.
  /// If [bufferSize] is not provided, the ReplaySubject will buffer all events.
  ReplaySubject<T> toReplaySubject({int? bufferSize}) {
    // Initialize the ReplaySubject with an optional buffer size
    final ReplaySubject<T> subject = bufferSize != null
        ? ReplaySubject<T>(maxSize: bufferSize)
        : ReplaySubject<T>();

    // Listen to the original stream and forward events to the ReplaySubject
    // CRITICAL FIX: Store subscription so it can be cancelled
    late StreamSubscription<T> subscription;
    subscription = listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () {
        subject.close();
        // No need to cancel subscription here as stream is done
      },
    );

    // CRITICAL FIX: Cancel subscription when subject is closed manually
    // This prevents memory leaks when the subject is disposed before stream completes
    subject.onCancel = () {
      subscription.cancel();
    };

    return subject;
  }
}

extension StreamCompletionFuture<T> on Stream<T> {
  Future<void> get completionFuture {
    final Completer<void> completer = Completer<void>();

    listen(
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

Future<T> select<T>(Iterable<Future<T>> futures, {Function? catchError}) async {
  var res = Stream<T>.fromFutures(futures).first;
  return await (catchError == null ? res : res.catchError(catchError));
}
