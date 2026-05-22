import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:rxdart/rxdart.dart';

Stream<T> rustBroadcastStream<T>({
  required Object Function(RustStreamSink<T> sink) attach,
  required bool Function(Object id) detach,
}) {
  final controller = StreamController<T>();
  RustStreamSink<T>? sink;
  StreamSubscription<T>? upstream;
  Object? id;
  var cancelled = false;
  var detached = false;
  void detachOnce() {
    if (detached) return;
    detached = true;
    if (id == null) return;
    try {
      final ok = detach(id!);
      assert(() {
        if (!ok) debugPrint('rustBroadcastStream detach returned false');
        return true;
      }());
    } catch (e, st) {
      assert(() {
        debugPrint('rustBroadcastStream detach failed: $e\n$st');
        return true;
      }());
    }
  }

  controller.onListen = () {
    sink = RustStreamSink<T>();
    // `RustStreamSink.stream` throws until `setupAndSerialize` runs, which
    // happens during attach's FRB serialization. Order must be attach first,
    // then listen. Anything Rust adds during attach (e.g. BehaviorBroadcast's
    // cached emit) lands in FRB's listenAndBuffer buffer and is replayed
    // when we subscribe.
    try {
      id = attach(sink!);
    } catch (e, st) {
      cancelled = true;
      controller.addError(e, st);
      controller.close();
      return;
    }
    upstream = sink!.stream.listen(
      (v) {
        if (!cancelled) controller.add(v);
      },
      onError: (Object e, StackTrace st) {
        if (!cancelled) controller.addError(e, st);
      },
      onDone: () {
        if (cancelled) return;
        detachOnce();
        controller.close();
      },
    );
  };
  controller.onCancel = () {
    cancelled = true;
    detachOnce();
    upstream?.cancel();
  };
  return controller.stream;
}

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
    listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () {
        subject.close();
      },
    );

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
    listen(
      (data) => subject.add(data),
      onError: (error) => subject.addError(error),
      onDone: () {
        subject.close();
      },
    );

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
