import 'dart:async';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:rxdart/rxdart.dart';

/// A stream of a Rust `Broadcast` that holds a Rust sink exactly while it has a listener.
///
/// `attach` registers the sink and returns its registration id; `detach` unregisters it. Backs the
/// `watch()` that `broadcast_handle!` generates.
Stream<T> rustBroadcastStream<T>({
  required Object Function(RustStreamSink<T> sink) attach,
  required void Function(Object id) detach,
}) {
  late final StreamController<T> controller;
  StreamSubscription<T>? upstream;
  Object? id;

  void detachOnce() {
    final attached = id;
    id = null;
    if (attached != null) detach(attached);
  }

  controller = StreamController<T>(
    onListen: () {
      final sink = RustStreamSink<T>();
      // `sink.stream` throws until attach has serialized the sink, so attach comes first.
      // Whatever Rust adds during attach (a BehaviorBroadcast's current value) is buffered by
      // frb and delivered once we listen.
      try {
        id = attach(sink);
      } catch (e, st) {
        controller.addError(e, st);
        controller.close();
        return;
      }
      upstream = sink.stream.listen(
        controller.add,
        onError: controller.addError,
        onDone: () {
          detachOnce();
          controller.close();
        },
      );
    },
    onCancel: () {
      detachOnce();
      return upstream?.cancel();
    },
  );
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
