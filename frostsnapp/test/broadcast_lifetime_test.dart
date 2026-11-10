import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/stream_ext.dart';

const _settle = Duration(milliseconds: 50);
const _cancelDeadline = Duration(seconds: 1);

/// The prefix of `Dart_CObject` (dart_native_api.h) that an int64 message uses.
final class _Int64CObject extends Struct {
  @Int32()
  external int type;
  @Int64()
  external int value;
}

const _kInt64 = 3;

final _postCObject = NativeApi.postCObject
    .asFunction<int Function(int port, Pointer<Dart_CObject> message)>();

/// Posts to a port the way Rust does, returning false once the port is closed.
bool _postInteger(int port, int value) => using((arena) {
  final message = arena<_Int64CObject>();
  message.ref
    ..type = _kInt64
    ..value = value;
  return _postCObject(port, message.cast()) != 0;
});

class _IntCodec extends DcoCodec<int, Object> {
  const _IntCodec()
    : super(decodeSuccessData: _decodeInt, decodeErrorData: null);

  static int _decodeInt(dynamic raw) => raw as int;

  // The fake posts bare ints, not the `[action, data]` lists frb's Rust side sends.
  @override
  int decodeObject(dynamic raw) => decodeSuccessData(raw);
}

/// Stands in for a Rust `Broadcast<i32>`, posting to each sink's port as Rust would.
class _FakeBroadcast {
  final ports = <int, int>{};
  var _nextId = 0;
  Object? detachError;

  int get subscriberCount => ports.length;

  Stream<int> watch() => rustBroadcastStream<int>(
    attach: (sink) {
      final id = _nextId++;
      ports[id] = int.parse(sink.setupAndSerialize(codec: const _IntCodec()));
      return id;
    },
    detach: (id) {
      if (detachError != null) throw detachError!;
      // Rust posts a close message when it drops the sink, and frb's stream only finishes
      // cancelling once a message wakes it.
      _postInteger(ports.remove(id)!, -1);
    },
  );

  void add(int value) {
    for (final port in ports.values) {
      _postInteger(port, value);
    }
  }
}

void main() {
  test('two listeners: cancelling the newer leaves the older', () async {
    final bcast = _FakeBroadcast();
    final a = <int>[];
    final b = <int>[];
    final subA = bcast.watch().listen(a.add);
    final subB = bcast.watch().listen(b.add);
    expect(bcast.subscriberCount, 2);

    bcast.add(1);
    await Future.delayed(_settle);
    expect(a, [1]);
    expect(b, [1]);

    await subB.cancel().timeout(_cancelDeadline);
    expect(bcast.subscriberCount, 1);

    bcast.add(2);
    await Future.delayed(_settle);
    expect(a, [1, 2], reason: 'remaining listener still receives');
    expect(b, [1], reason: 'cancelled listener gets nothing');

    await subA.cancel().timeout(_cancelDeadline);
    expect(bcast.subscriberCount, 0);
  });

  test('nothing attaches before listen', () {
    final bcast = _FakeBroadcast();
    bcast.watch();
    expect(bcast.subscriberCount, 0);
  });

  test('a throwing detach fails the cancel and still closes the port', () async {
    final bcast = _FakeBroadcast();
    final sub = bcast.watch().listen((_) {});
    final port = bcast.ports.values.single;

    bcast.detachError = StateError('poisoned');
    await expectLater(sub.cancel(), throwsA(bcast.detachError));

    // Still attached in Rust, so its next add is what wakes frb's stream to finish closing.
    bcast.add(1);
    await Future.delayed(_settle);
    expect(_postInteger(port, 2), isFalse, reason: 'port is closed');
  });
}
