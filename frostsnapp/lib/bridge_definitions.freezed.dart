// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'bridge_definitions.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#custom-getters-and-methods');

/// @nodoc
mixin _$CoordinatorToUserKeyGenMessage {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId from) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId from) keyGenAck,
    required TResult Function(KeyId keyId) finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId from)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId from)? keyGenAck,
    TResult? Function(KeyId keyId)? finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId from)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId from)? keyGenAck,
    TResult Function(KeyId keyId)? finishedKey,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(
            CoordinatorToUserKeyGenMessage_ReceivedShares value)
        receivedShares,
    required TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)
        checkKeyGen,
    required TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)
        keyGenAck,
    required TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)
        finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult? Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult? Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)?
        keyGenAck,
    TResult? Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)? keyGenAck,
    TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $CoordinatorToUserKeyGenMessageCopyWith<$Res> {
  factory $CoordinatorToUserKeyGenMessageCopyWith(
          CoordinatorToUserKeyGenMessage value,
          $Res Function(CoordinatorToUserKeyGenMessage) then) =
      _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
          CoordinatorToUserKeyGenMessage>;
}

/// @nodoc
class _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        $Val extends CoordinatorToUserKeyGenMessage>
    implements $CoordinatorToUserKeyGenMessageCopyWith<$Res> {
  _$CoordinatorToUserKeyGenMessageCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWith(
          _$CoordinatorToUserKeyGenMessage_ReceivedShares value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_ReceivedShares) then) =
      __$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWithImpl<$Res>;
  @useResult
  $Res call({DeviceId from});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_ReceivedShares>
    implements _$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_ReceivedShares _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_ReceivedShares) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? from = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_ReceivedShares(
      from: null == from
          ? _value.from
          : from // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_ReceivedShares
    implements CoordinatorToUserKeyGenMessage_ReceivedShares {
  const _$CoordinatorToUserKeyGenMessage_ReceivedShares({required this.from});

  @override
  final DeviceId from;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.receivedShares(from: $from)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_ReceivedShares &&
            (identical(other.from, from) || other.from == from));
  }

  @override
  int get hashCode => Object.hash(runtimeType, from);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWith<
          _$CoordinatorToUserKeyGenMessage_ReceivedShares>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWithImpl<
                  _$CoordinatorToUserKeyGenMessage_ReceivedShares>(
              this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId from) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId from) keyGenAck,
    required TResult Function(KeyId keyId) finishedKey,
  }) {
    return receivedShares(from);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId from)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId from)? keyGenAck,
    TResult? Function(KeyId keyId)? finishedKey,
  }) {
    return receivedShares?.call(from);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId from)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId from)? keyGenAck,
    TResult Function(KeyId keyId)? finishedKey,
    required TResult orElse(),
  }) {
    if (receivedShares != null) {
      return receivedShares(from);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(
            CoordinatorToUserKeyGenMessage_ReceivedShares value)
        receivedShares,
    required TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)
        checkKeyGen,
    required TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)
        keyGenAck,
    required TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)
        finishedKey,
  }) {
    return receivedShares(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult? Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult? Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)?
        keyGenAck,
    TResult? Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
  }) {
    return receivedShares?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)? keyGenAck,
    TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
    required TResult orElse(),
  }) {
    if (receivedShares != null) {
      return receivedShares(this);
    }
    return orElse();
  }
}

abstract class CoordinatorToUserKeyGenMessage_ReceivedShares
    implements CoordinatorToUserKeyGenMessage {
  const factory CoordinatorToUserKeyGenMessage_ReceivedShares(
          {required final DeviceId from}) =
      _$CoordinatorToUserKeyGenMessage_ReceivedShares;

  DeviceId get from;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_ReceivedSharesCopyWith<
          _$CoordinatorToUserKeyGenMessage_ReceivedShares>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWith(
          _$CoordinatorToUserKeyGenMessage_CheckKeyGen value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_CheckKeyGen) then) =
      __$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWithImpl<$Res>;
  @useResult
  $Res call({U8Array32 sessionHash});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_CheckKeyGen>
    implements _$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_CheckKeyGen _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_CheckKeyGen) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? sessionHash = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_CheckKeyGen(
      sessionHash: null == sessionHash
          ? _value.sessionHash
          : sessionHash // ignore: cast_nullable_to_non_nullable
              as U8Array32,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_CheckKeyGen
    implements CoordinatorToUserKeyGenMessage_CheckKeyGen {
  const _$CoordinatorToUserKeyGenMessage_CheckKeyGen(
      {required this.sessionHash});

  @override
  final U8Array32 sessionHash;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.checkKeyGen(sessionHash: $sessionHash)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_CheckKeyGen &&
            const DeepCollectionEquality()
                .equals(other.sessionHash, sessionHash));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType, const DeepCollectionEquality().hash(sessionHash));

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWith<
          _$CoordinatorToUserKeyGenMessage_CheckKeyGen>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWithImpl<
              _$CoordinatorToUserKeyGenMessage_CheckKeyGen>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId from) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId from) keyGenAck,
    required TResult Function(KeyId keyId) finishedKey,
  }) {
    return checkKeyGen(sessionHash);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId from)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId from)? keyGenAck,
    TResult? Function(KeyId keyId)? finishedKey,
  }) {
    return checkKeyGen?.call(sessionHash);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId from)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId from)? keyGenAck,
    TResult Function(KeyId keyId)? finishedKey,
    required TResult orElse(),
  }) {
    if (checkKeyGen != null) {
      return checkKeyGen(sessionHash);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(
            CoordinatorToUserKeyGenMessage_ReceivedShares value)
        receivedShares,
    required TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)
        checkKeyGen,
    required TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)
        keyGenAck,
    required TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)
        finishedKey,
  }) {
    return checkKeyGen(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult? Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult? Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)?
        keyGenAck,
    TResult? Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
  }) {
    return checkKeyGen?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)? keyGenAck,
    TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
    required TResult orElse(),
  }) {
    if (checkKeyGen != null) {
      return checkKeyGen(this);
    }
    return orElse();
  }
}

abstract class CoordinatorToUserKeyGenMessage_CheckKeyGen
    implements CoordinatorToUserKeyGenMessage {
  const factory CoordinatorToUserKeyGenMessage_CheckKeyGen(
          {required final U8Array32 sessionHash}) =
      _$CoordinatorToUserKeyGenMessage_CheckKeyGen;

  U8Array32 get sessionHash;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_CheckKeyGenCopyWith<
          _$CoordinatorToUserKeyGenMessage_CheckKeyGen>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWith(
          _$CoordinatorToUserKeyGenMessage_KeyGenAck value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_KeyGenAck) then) =
      __$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWithImpl<$Res>;
  @useResult
  $Res call({DeviceId from});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_KeyGenAck>
    implements _$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_KeyGenAck _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_KeyGenAck) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? from = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_KeyGenAck(
      from: null == from
          ? _value.from
          : from // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_KeyGenAck
    implements CoordinatorToUserKeyGenMessage_KeyGenAck {
  const _$CoordinatorToUserKeyGenMessage_KeyGenAck({required this.from});

  @override
  final DeviceId from;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.keyGenAck(from: $from)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_KeyGenAck &&
            (identical(other.from, from) || other.from == from));
  }

  @override
  int get hashCode => Object.hash(runtimeType, from);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWith<
          _$CoordinatorToUserKeyGenMessage_KeyGenAck>
      get copyWith => __$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWithImpl<
          _$CoordinatorToUserKeyGenMessage_KeyGenAck>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId from) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId from) keyGenAck,
    required TResult Function(KeyId keyId) finishedKey,
  }) {
    return keyGenAck(from);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId from)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId from)? keyGenAck,
    TResult? Function(KeyId keyId)? finishedKey,
  }) {
    return keyGenAck?.call(from);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId from)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId from)? keyGenAck,
    TResult Function(KeyId keyId)? finishedKey,
    required TResult orElse(),
  }) {
    if (keyGenAck != null) {
      return keyGenAck(from);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(
            CoordinatorToUserKeyGenMessage_ReceivedShares value)
        receivedShares,
    required TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)
        checkKeyGen,
    required TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)
        keyGenAck,
    required TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)
        finishedKey,
  }) {
    return keyGenAck(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult? Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult? Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)?
        keyGenAck,
    TResult? Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
  }) {
    return keyGenAck?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)? keyGenAck,
    TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
    required TResult orElse(),
  }) {
    if (keyGenAck != null) {
      return keyGenAck(this);
    }
    return orElse();
  }
}

abstract class CoordinatorToUserKeyGenMessage_KeyGenAck
    implements CoordinatorToUserKeyGenMessage {
  const factory CoordinatorToUserKeyGenMessage_KeyGenAck(
          {required final DeviceId from}) =
      _$CoordinatorToUserKeyGenMessage_KeyGenAck;

  DeviceId get from;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_KeyGenAckCopyWith<
          _$CoordinatorToUserKeyGenMessage_KeyGenAck>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWith(
          _$CoordinatorToUserKeyGenMessage_FinishedKey value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_FinishedKey) then) =
      __$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWithImpl<$Res>;
  @useResult
  $Res call({KeyId keyId});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_FinishedKey>
    implements _$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_FinishedKey _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_FinishedKey) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? keyId = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_FinishedKey(
      keyId: null == keyId
          ? _value.keyId
          : keyId // ignore: cast_nullable_to_non_nullable
              as KeyId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_FinishedKey
    implements CoordinatorToUserKeyGenMessage_FinishedKey {
  const _$CoordinatorToUserKeyGenMessage_FinishedKey({required this.keyId});

  @override
  final KeyId keyId;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.finishedKey(keyId: $keyId)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_FinishedKey &&
            (identical(other.keyId, keyId) || other.keyId == keyId));
  }

  @override
  int get hashCode => Object.hash(runtimeType, keyId);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWith<
          _$CoordinatorToUserKeyGenMessage_FinishedKey>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWithImpl<
              _$CoordinatorToUserKeyGenMessage_FinishedKey>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId from) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId from) keyGenAck,
    required TResult Function(KeyId keyId) finishedKey,
  }) {
    return finishedKey(keyId);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId from)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId from)? keyGenAck,
    TResult? Function(KeyId keyId)? finishedKey,
  }) {
    return finishedKey?.call(keyId);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId from)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId from)? keyGenAck,
    TResult Function(KeyId keyId)? finishedKey,
    required TResult orElse(),
  }) {
    if (finishedKey != null) {
      return finishedKey(keyId);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(
            CoordinatorToUserKeyGenMessage_ReceivedShares value)
        receivedShares,
    required TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)
        checkKeyGen,
    required TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)
        keyGenAck,
    required TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)
        finishedKey,
  }) {
    return finishedKey(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult? Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult? Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)?
        keyGenAck,
    TResult? Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
  }) {
    return finishedKey?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorToUserKeyGenMessage_ReceivedShares value)?
        receivedShares,
    TResult Function(CoordinatorToUserKeyGenMessage_CheckKeyGen value)?
        checkKeyGen,
    TResult Function(CoordinatorToUserKeyGenMessage_KeyGenAck value)? keyGenAck,
    TResult Function(CoordinatorToUserKeyGenMessage_FinishedKey value)?
        finishedKey,
    required TResult orElse(),
  }) {
    if (finishedKey != null) {
      return finishedKey(this);
    }
    return orElse();
  }
}

abstract class CoordinatorToUserKeyGenMessage_FinishedKey
    implements CoordinatorToUserKeyGenMessage {
  const factory CoordinatorToUserKeyGenMessage_FinishedKey(
          {required final KeyId keyId}) =
      _$CoordinatorToUserKeyGenMessage_FinishedKey;

  KeyId get keyId;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_FinishedKeyCopyWith<
          _$CoordinatorToUserKeyGenMessage_FinishedKey>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$PortEvent {
  Object get request => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) open,
    required TResult Function(PortWrite request) write,
    required TResult Function(PortRead request) read,
    required TResult Function(PortBytesToRead request) bytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? open,
    TResult? Function(PortWrite request)? write,
    TResult? Function(PortRead request)? read,
    TResult? Function(PortBytesToRead request)? bytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? open,
    TResult Function(PortWrite request)? write,
    TResult Function(PortRead request)? read,
    TResult Function(PortBytesToRead request)? bytesToRead,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PortEvent_Open value) open,
    required TResult Function(PortEvent_Write value) write,
    required TResult Function(PortEvent_Read value) read,
    required TResult Function(PortEvent_BytesToRead value) bytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PortEvent_Open value)? open,
    TResult? Function(PortEvent_Write value)? write,
    TResult? Function(PortEvent_Read value)? read,
    TResult? Function(PortEvent_BytesToRead value)? bytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PortEvent_Open value)? open,
    TResult Function(PortEvent_Write value)? write,
    TResult Function(PortEvent_Read value)? read,
    TResult Function(PortEvent_BytesToRead value)? bytesToRead,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $PortEventCopyWith<$Res> {
  factory $PortEventCopyWith(PortEvent value, $Res Function(PortEvent) then) =
      _$PortEventCopyWithImpl<$Res, PortEvent>;
}

/// @nodoc
class _$PortEventCopyWithImpl<$Res, $Val extends PortEvent>
    implements $PortEventCopyWith<$Res> {
  _$PortEventCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$PortEvent_OpenCopyWith<$Res> {
  factory _$$PortEvent_OpenCopyWith(
          _$PortEvent_Open value, $Res Function(_$PortEvent_Open) then) =
      __$$PortEvent_OpenCopyWithImpl<$Res>;
  @useResult
  $Res call({PortOpen request});
}

/// @nodoc
class __$$PortEvent_OpenCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_Open>
    implements _$$PortEvent_OpenCopyWith<$Res> {
  __$$PortEvent_OpenCopyWithImpl(
      _$PortEvent_Open _value, $Res Function(_$PortEvent_Open) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_Open(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortOpen,
    ));
  }
}

/// @nodoc

class _$PortEvent_Open implements PortEvent_Open {
  const _$PortEvent_Open({required this.request});

  @override
  final PortOpen request;

  @override
  String toString() {
    return 'PortEvent.open(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_Open &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_OpenCopyWith<_$PortEvent_Open> get copyWith =>
      __$$PortEvent_OpenCopyWithImpl<_$PortEvent_Open>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) open,
    required TResult Function(PortWrite request) write,
    required TResult Function(PortRead request) read,
    required TResult Function(PortBytesToRead request) bytesToRead,
  }) {
    return open(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? open,
    TResult? Function(PortWrite request)? write,
    TResult? Function(PortRead request)? read,
    TResult? Function(PortBytesToRead request)? bytesToRead,
  }) {
    return open?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? open,
    TResult Function(PortWrite request)? write,
    TResult Function(PortRead request)? read,
    TResult Function(PortBytesToRead request)? bytesToRead,
    required TResult orElse(),
  }) {
    if (open != null) {
      return open(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PortEvent_Open value) open,
    required TResult Function(PortEvent_Write value) write,
    required TResult Function(PortEvent_Read value) read,
    required TResult Function(PortEvent_BytesToRead value) bytesToRead,
  }) {
    return open(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PortEvent_Open value)? open,
    TResult? Function(PortEvent_Write value)? write,
    TResult? Function(PortEvent_Read value)? read,
    TResult? Function(PortEvent_BytesToRead value)? bytesToRead,
  }) {
    return open?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PortEvent_Open value)? open,
    TResult Function(PortEvent_Write value)? write,
    TResult Function(PortEvent_Read value)? read,
    TResult Function(PortEvent_BytesToRead value)? bytesToRead,
    required TResult orElse(),
  }) {
    if (open != null) {
      return open(this);
    }
    return orElse();
  }
}

abstract class PortEvent_Open implements PortEvent {
  const factory PortEvent_Open({required final PortOpen request}) =
      _$PortEvent_Open;

  @override
  PortOpen get request;
  @JsonKey(ignore: true)
  _$$PortEvent_OpenCopyWith<_$PortEvent_Open> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_WriteCopyWith<$Res> {
  factory _$$PortEvent_WriteCopyWith(
          _$PortEvent_Write value, $Res Function(_$PortEvent_Write) then) =
      __$$PortEvent_WriteCopyWithImpl<$Res>;
  @useResult
  $Res call({PortWrite request});
}

/// @nodoc
class __$$PortEvent_WriteCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_Write>
    implements _$$PortEvent_WriteCopyWith<$Res> {
  __$$PortEvent_WriteCopyWithImpl(
      _$PortEvent_Write _value, $Res Function(_$PortEvent_Write) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_Write(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortWrite,
    ));
  }
}

/// @nodoc

class _$PortEvent_Write implements PortEvent_Write {
  const _$PortEvent_Write({required this.request});

  @override
  final PortWrite request;

  @override
  String toString() {
    return 'PortEvent.write(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_Write &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_WriteCopyWith<_$PortEvent_Write> get copyWith =>
      __$$PortEvent_WriteCopyWithImpl<_$PortEvent_Write>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) open,
    required TResult Function(PortWrite request) write,
    required TResult Function(PortRead request) read,
    required TResult Function(PortBytesToRead request) bytesToRead,
  }) {
    return write(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? open,
    TResult? Function(PortWrite request)? write,
    TResult? Function(PortRead request)? read,
    TResult? Function(PortBytesToRead request)? bytesToRead,
  }) {
    return write?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? open,
    TResult Function(PortWrite request)? write,
    TResult Function(PortRead request)? read,
    TResult Function(PortBytesToRead request)? bytesToRead,
    required TResult orElse(),
  }) {
    if (write != null) {
      return write(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PortEvent_Open value) open,
    required TResult Function(PortEvent_Write value) write,
    required TResult Function(PortEvent_Read value) read,
    required TResult Function(PortEvent_BytesToRead value) bytesToRead,
  }) {
    return write(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PortEvent_Open value)? open,
    TResult? Function(PortEvent_Write value)? write,
    TResult? Function(PortEvent_Read value)? read,
    TResult? Function(PortEvent_BytesToRead value)? bytesToRead,
  }) {
    return write?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PortEvent_Open value)? open,
    TResult Function(PortEvent_Write value)? write,
    TResult Function(PortEvent_Read value)? read,
    TResult Function(PortEvent_BytesToRead value)? bytesToRead,
    required TResult orElse(),
  }) {
    if (write != null) {
      return write(this);
    }
    return orElse();
  }
}

abstract class PortEvent_Write implements PortEvent {
  const factory PortEvent_Write({required final PortWrite request}) =
      _$PortEvent_Write;

  @override
  PortWrite get request;
  @JsonKey(ignore: true)
  _$$PortEvent_WriteCopyWith<_$PortEvent_Write> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_ReadCopyWith<$Res> {
  factory _$$PortEvent_ReadCopyWith(
          _$PortEvent_Read value, $Res Function(_$PortEvent_Read) then) =
      __$$PortEvent_ReadCopyWithImpl<$Res>;
  @useResult
  $Res call({PortRead request});
}

/// @nodoc
class __$$PortEvent_ReadCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_Read>
    implements _$$PortEvent_ReadCopyWith<$Res> {
  __$$PortEvent_ReadCopyWithImpl(
      _$PortEvent_Read _value, $Res Function(_$PortEvent_Read) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_Read(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortRead,
    ));
  }
}

/// @nodoc

class _$PortEvent_Read implements PortEvent_Read {
  const _$PortEvent_Read({required this.request});

  @override
  final PortRead request;

  @override
  String toString() {
    return 'PortEvent.read(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_Read &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_ReadCopyWith<_$PortEvent_Read> get copyWith =>
      __$$PortEvent_ReadCopyWithImpl<_$PortEvent_Read>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) open,
    required TResult Function(PortWrite request) write,
    required TResult Function(PortRead request) read,
    required TResult Function(PortBytesToRead request) bytesToRead,
  }) {
    return read(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? open,
    TResult? Function(PortWrite request)? write,
    TResult? Function(PortRead request)? read,
    TResult? Function(PortBytesToRead request)? bytesToRead,
  }) {
    return read?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? open,
    TResult Function(PortWrite request)? write,
    TResult Function(PortRead request)? read,
    TResult Function(PortBytesToRead request)? bytesToRead,
    required TResult orElse(),
  }) {
    if (read != null) {
      return read(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PortEvent_Open value) open,
    required TResult Function(PortEvent_Write value) write,
    required TResult Function(PortEvent_Read value) read,
    required TResult Function(PortEvent_BytesToRead value) bytesToRead,
  }) {
    return read(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PortEvent_Open value)? open,
    TResult? Function(PortEvent_Write value)? write,
    TResult? Function(PortEvent_Read value)? read,
    TResult? Function(PortEvent_BytesToRead value)? bytesToRead,
  }) {
    return read?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PortEvent_Open value)? open,
    TResult Function(PortEvent_Write value)? write,
    TResult Function(PortEvent_Read value)? read,
    TResult Function(PortEvent_BytesToRead value)? bytesToRead,
    required TResult orElse(),
  }) {
    if (read != null) {
      return read(this);
    }
    return orElse();
  }
}

abstract class PortEvent_Read implements PortEvent {
  const factory PortEvent_Read({required final PortRead request}) =
      _$PortEvent_Read;

  @override
  PortRead get request;
  @JsonKey(ignore: true)
  _$$PortEvent_ReadCopyWith<_$PortEvent_Read> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_BytesToReadCopyWith<$Res> {
  factory _$$PortEvent_BytesToReadCopyWith(_$PortEvent_BytesToRead value,
          $Res Function(_$PortEvent_BytesToRead) then) =
      __$$PortEvent_BytesToReadCopyWithImpl<$Res>;
  @useResult
  $Res call({PortBytesToRead request});
}

/// @nodoc
class __$$PortEvent_BytesToReadCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_BytesToRead>
    implements _$$PortEvent_BytesToReadCopyWith<$Res> {
  __$$PortEvent_BytesToReadCopyWithImpl(_$PortEvent_BytesToRead _value,
      $Res Function(_$PortEvent_BytesToRead) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_BytesToRead(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortBytesToRead,
    ));
  }
}

/// @nodoc

class _$PortEvent_BytesToRead implements PortEvent_BytesToRead {
  const _$PortEvent_BytesToRead({required this.request});

  @override
  final PortBytesToRead request;

  @override
  String toString() {
    return 'PortEvent.bytesToRead(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_BytesToRead &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_BytesToReadCopyWith<_$PortEvent_BytesToRead> get copyWith =>
      __$$PortEvent_BytesToReadCopyWithImpl<_$PortEvent_BytesToRead>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) open,
    required TResult Function(PortWrite request) write,
    required TResult Function(PortRead request) read,
    required TResult Function(PortBytesToRead request) bytesToRead,
  }) {
    return bytesToRead(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? open,
    TResult? Function(PortWrite request)? write,
    TResult? Function(PortRead request)? read,
    TResult? Function(PortBytesToRead request)? bytesToRead,
  }) {
    return bytesToRead?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? open,
    TResult Function(PortWrite request)? write,
    TResult Function(PortRead request)? read,
    TResult Function(PortBytesToRead request)? bytesToRead,
    required TResult orElse(),
  }) {
    if (bytesToRead != null) {
      return bytesToRead(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(PortEvent_Open value) open,
    required TResult Function(PortEvent_Write value) write,
    required TResult Function(PortEvent_Read value) read,
    required TResult Function(PortEvent_BytesToRead value) bytesToRead,
  }) {
    return bytesToRead(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(PortEvent_Open value)? open,
    TResult? Function(PortEvent_Write value)? write,
    TResult? Function(PortEvent_Read value)? read,
    TResult? Function(PortEvent_BytesToRead value)? bytesToRead,
  }) {
    return bytesToRead?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(PortEvent_Open value)? open,
    TResult Function(PortEvent_Write value)? write,
    TResult Function(PortEvent_Read value)? read,
    TResult Function(PortEvent_BytesToRead value)? bytesToRead,
    required TResult orElse(),
  }) {
    if (bytesToRead != null) {
      return bytesToRead(this);
    }
    return orElse();
  }
}

abstract class PortEvent_BytesToRead implements PortEvent {
  const factory PortEvent_BytesToRead(
      {required final PortBytesToRead request}) = _$PortEvent_BytesToRead;

  @override
  PortBytesToRead get request;
  @JsonKey(ignore: true)
  _$$PortEvent_BytesToReadCopyWith<_$PortEvent_BytesToRead> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$SignTaskDescription {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedNostrEvent unsignedEvent) nostr,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
    TResult Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult Function(UnsignedTx unsignedTx)? transaction,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SignTaskDescription_Plain value) plain,
    required TResult Function(SignTaskDescription_Nostr value) nostr,
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Nostr value)? nostr,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
    TResult Function(SignTaskDescription_Nostr value)? nostr,
    TResult Function(SignTaskDescription_Transaction value)? transaction,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $SignTaskDescriptionCopyWith<$Res> {
  factory $SignTaskDescriptionCopyWith(
          SignTaskDescription value, $Res Function(SignTaskDescription) then) =
      _$SignTaskDescriptionCopyWithImpl<$Res, SignTaskDescription>;
}

/// @nodoc
class _$SignTaskDescriptionCopyWithImpl<$Res, $Val extends SignTaskDescription>
    implements $SignTaskDescriptionCopyWith<$Res> {
  _$SignTaskDescriptionCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$SignTaskDescription_PlainCopyWith<$Res> {
  factory _$$SignTaskDescription_PlainCopyWith(
          _$SignTaskDescription_Plain value,
          $Res Function(_$SignTaskDescription_Plain) then) =
      __$$SignTaskDescription_PlainCopyWithImpl<$Res>;
  @useResult
  $Res call({String message});
}

/// @nodoc
class __$$SignTaskDescription_PlainCopyWithImpl<$Res>
    extends _$SignTaskDescriptionCopyWithImpl<$Res, _$SignTaskDescription_Plain>
    implements _$$SignTaskDescription_PlainCopyWith<$Res> {
  __$$SignTaskDescription_PlainCopyWithImpl(_$SignTaskDescription_Plain _value,
      $Res Function(_$SignTaskDescription_Plain) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? message = null,
  }) {
    return _then(_$SignTaskDescription_Plain(
      message: null == message
          ? _value.message
          : message // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SignTaskDescription_Plain implements SignTaskDescription_Plain {
  const _$SignTaskDescription_Plain({required this.message});

  @override
  final String message;

  @override
  String toString() {
    return 'SignTaskDescription.plain(message: $message)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SignTaskDescription_Plain &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode => Object.hash(runtimeType, message);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$SignTaskDescription_PlainCopyWith<_$SignTaskDescription_Plain>
      get copyWith => __$$SignTaskDescription_PlainCopyWithImpl<
          _$SignTaskDescription_Plain>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedNostrEvent unsignedEvent) nostr,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) {
    return plain(message);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) {
    return plain?.call(message);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
    TResult Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult Function(UnsignedTx unsignedTx)? transaction,
    required TResult orElse(),
  }) {
    if (plain != null) {
      return plain(message);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SignTaskDescription_Plain value) plain,
    required TResult Function(SignTaskDescription_Nostr value) nostr,
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) {
    return plain(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Nostr value)? nostr,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) {
    return plain?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
    TResult Function(SignTaskDescription_Nostr value)? nostr,
    TResult Function(SignTaskDescription_Transaction value)? transaction,
    required TResult orElse(),
  }) {
    if (plain != null) {
      return plain(this);
    }
    return orElse();
  }
}

abstract class SignTaskDescription_Plain implements SignTaskDescription {
  const factory SignTaskDescription_Plain({required final String message}) =
      _$SignTaskDescription_Plain;

  String get message;
  @JsonKey(ignore: true)
  _$$SignTaskDescription_PlainCopyWith<_$SignTaskDescription_Plain>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SignTaskDescription_NostrCopyWith<$Res> {
  factory _$$SignTaskDescription_NostrCopyWith(
          _$SignTaskDescription_Nostr value,
          $Res Function(_$SignTaskDescription_Nostr) then) =
      __$$SignTaskDescription_NostrCopyWithImpl<$Res>;
  @useResult
  $Res call({UnsignedNostrEvent unsignedEvent});
}

/// @nodoc
class __$$SignTaskDescription_NostrCopyWithImpl<$Res>
    extends _$SignTaskDescriptionCopyWithImpl<$Res, _$SignTaskDescription_Nostr>
    implements _$$SignTaskDescription_NostrCopyWith<$Res> {
  __$$SignTaskDescription_NostrCopyWithImpl(_$SignTaskDescription_Nostr _value,
      $Res Function(_$SignTaskDescription_Nostr) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? unsignedEvent = null,
  }) {
    return _then(_$SignTaskDescription_Nostr(
      unsignedEvent: null == unsignedEvent
          ? _value.unsignedEvent
          : unsignedEvent // ignore: cast_nullable_to_non_nullable
              as UnsignedNostrEvent,
    ));
  }
}

/// @nodoc

class _$SignTaskDescription_Nostr implements SignTaskDescription_Nostr {
  const _$SignTaskDescription_Nostr({required this.unsignedEvent});

  @override
  final UnsignedNostrEvent unsignedEvent;

  @override
  String toString() {
    return 'SignTaskDescription.nostr(unsignedEvent: $unsignedEvent)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SignTaskDescription_Nostr &&
            (identical(other.unsignedEvent, unsignedEvent) ||
                other.unsignedEvent == unsignedEvent));
  }

  @override
  int get hashCode => Object.hash(runtimeType, unsignedEvent);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$SignTaskDescription_NostrCopyWith<_$SignTaskDescription_Nostr>
      get copyWith => __$$SignTaskDescription_NostrCopyWithImpl<
          _$SignTaskDescription_Nostr>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedNostrEvent unsignedEvent) nostr,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) {
    return nostr(unsignedEvent);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) {
    return nostr?.call(unsignedEvent);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
    TResult Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult Function(UnsignedTx unsignedTx)? transaction,
    required TResult orElse(),
  }) {
    if (nostr != null) {
      return nostr(unsignedEvent);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SignTaskDescription_Plain value) plain,
    required TResult Function(SignTaskDescription_Nostr value) nostr,
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) {
    return nostr(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Nostr value)? nostr,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) {
    return nostr?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
    TResult Function(SignTaskDescription_Nostr value)? nostr,
    TResult Function(SignTaskDescription_Transaction value)? transaction,
    required TResult orElse(),
  }) {
    if (nostr != null) {
      return nostr(this);
    }
    return orElse();
  }
}

abstract class SignTaskDescription_Nostr implements SignTaskDescription {
  const factory SignTaskDescription_Nostr(
          {required final UnsignedNostrEvent unsignedEvent}) =
      _$SignTaskDescription_Nostr;

  UnsignedNostrEvent get unsignedEvent;
  @JsonKey(ignore: true)
  _$$SignTaskDescription_NostrCopyWith<_$SignTaskDescription_Nostr>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SignTaskDescription_TransactionCopyWith<$Res> {
  factory _$$SignTaskDescription_TransactionCopyWith(
          _$SignTaskDescription_Transaction value,
          $Res Function(_$SignTaskDescription_Transaction) then) =
      __$$SignTaskDescription_TransactionCopyWithImpl<$Res>;
  @useResult
  $Res call({UnsignedTx unsignedTx});
}

/// @nodoc
class __$$SignTaskDescription_TransactionCopyWithImpl<$Res>
    extends _$SignTaskDescriptionCopyWithImpl<$Res,
        _$SignTaskDescription_Transaction>
    implements _$$SignTaskDescription_TransactionCopyWith<$Res> {
  __$$SignTaskDescription_TransactionCopyWithImpl(
      _$SignTaskDescription_Transaction _value,
      $Res Function(_$SignTaskDescription_Transaction) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? unsignedTx = null,
  }) {
    return _then(_$SignTaskDescription_Transaction(
      unsignedTx: null == unsignedTx
          ? _value.unsignedTx
          : unsignedTx // ignore: cast_nullable_to_non_nullable
              as UnsignedTx,
    ));
  }
}

/// @nodoc

class _$SignTaskDescription_Transaction
    implements SignTaskDescription_Transaction {
  const _$SignTaskDescription_Transaction({required this.unsignedTx});

  @override
  final UnsignedTx unsignedTx;

  @override
  String toString() {
    return 'SignTaskDescription.transaction(unsignedTx: $unsignedTx)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SignTaskDescription_Transaction &&
            (identical(other.unsignedTx, unsignedTx) ||
                other.unsignedTx == unsignedTx));
  }

  @override
  int get hashCode => Object.hash(runtimeType, unsignedTx);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$SignTaskDescription_TransactionCopyWith<_$SignTaskDescription_Transaction>
      get copyWith => __$$SignTaskDescription_TransactionCopyWithImpl<
          _$SignTaskDescription_Transaction>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedNostrEvent unsignedEvent) nostr,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) {
    return transaction(unsignedTx);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) {
    return transaction?.call(unsignedTx);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
    TResult Function(UnsignedNostrEvent unsignedEvent)? nostr,
    TResult Function(UnsignedTx unsignedTx)? transaction,
    required TResult orElse(),
  }) {
    if (transaction != null) {
      return transaction(unsignedTx);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SignTaskDescription_Plain value) plain,
    required TResult Function(SignTaskDescription_Nostr value) nostr,
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) {
    return transaction(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Nostr value)? nostr,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) {
    return transaction?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
    TResult Function(SignTaskDescription_Nostr value)? nostr,
    TResult Function(SignTaskDescription_Transaction value)? transaction,
    required TResult orElse(),
  }) {
    if (transaction != null) {
      return transaction(this);
    }
    return orElse();
  }
}

abstract class SignTaskDescription_Transaction implements SignTaskDescription {
  const factory SignTaskDescription_Transaction(
          {required final UnsignedTx unsignedTx}) =
      _$SignTaskDescription_Transaction;

  UnsignedTx get unsignedTx;
  @JsonKey(ignore: true)
  _$$SignTaskDescription_TransactionCopyWith<_$SignTaskDescription_Transaction>
      get copyWith => throw _privateConstructorUsedError;
}
