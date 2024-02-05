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
abstract class _$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWith<
    $Res> {
  factory _$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWith(
          _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl)
              then) =
      __$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWithImpl<$Res>;
  @useResult
  $Res call({DeviceId from});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl>
    implements
        _$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? from = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl(
      from: null == from
          ? _value.from
          : from // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl
    implements CoordinatorToUserKeyGenMessage_ReceivedShares {
  const _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl(
      {required this.from});

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
            other is _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl &&
            (identical(other.from, from) || other.from == from));
  }

  @override
  int get hashCode => Object.hash(runtimeType, from);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWithImpl<
                  _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl>(
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
      _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl;

  DeviceId get from;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_ReceivedSharesImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_ReceivedSharesImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWith(
          _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl)
              then) =
      __$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWithImpl<$Res>;
  @useResult
  $Res call({U8Array32 sessionHash});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl>
    implements _$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? sessionHash = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl(
      sessionHash: null == sessionHash
          ? _value.sessionHash
          : sessionHash // ignore: cast_nullable_to_non_nullable
              as U8Array32,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl
    implements CoordinatorToUserKeyGenMessage_CheckKeyGen {
  const _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl(
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
            other is _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl &&
            const DeepCollectionEquality()
                .equals(other.sessionHash, sessionHash));
  }

  @override
  int get hashCode => Object.hash(
      runtimeType, const DeepCollectionEquality().hash(sessionHash));

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWithImpl<
                  _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl>(
              this, _$identity);

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
      _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl;

  U8Array32 get sessionHash;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_CheckKeyGenImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_CheckKeyGenImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWith(
          _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_KeyGenAckImpl) then) =
      __$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWithImpl<$Res>;
  @useResult
  $Res call({DeviceId from});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl>
    implements _$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_KeyGenAckImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? from = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_KeyGenAckImpl(
      from: null == from
          ? _value.from
          : from // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl
    implements CoordinatorToUserKeyGenMessage_KeyGenAck {
  const _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl({required this.from});

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
            other is _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl &&
            (identical(other.from, from) || other.from == from));
  }

  @override
  int get hashCode => Object.hash(runtimeType, from);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWithImpl<
              _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl>(this, _$identity);

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
      _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl;

  DeviceId get from;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_KeyGenAckImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_KeyGenAckImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWith<$Res> {
  factory _$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWith(
          _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl value,
          $Res Function(_$CoordinatorToUserKeyGenMessage_FinishedKeyImpl)
              then) =
      __$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWithImpl<$Res>;
  @useResult
  $Res call({KeyId keyId});
}

/// @nodoc
class __$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWithImpl<$Res>
    extends _$CoordinatorToUserKeyGenMessageCopyWithImpl<$Res,
        _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl>
    implements _$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWith<$Res> {
  __$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWithImpl(
      _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl _value,
      $Res Function(_$CoordinatorToUserKeyGenMessage_FinishedKeyImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? keyId = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_FinishedKeyImpl(
      keyId: null == keyId
          ? _value.keyId
          : keyId // ignore: cast_nullable_to_non_nullable
              as KeyId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl
    implements CoordinatorToUserKeyGenMessage_FinishedKey {
  const _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl({required this.keyId});

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
            other is _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl &&
            (identical(other.keyId, keyId) || other.keyId == keyId));
  }

  @override
  int get hashCode => Object.hash(runtimeType, keyId);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl>
      get copyWith =>
          __$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWithImpl<
                  _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl>(
              this, _$identity);

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
      _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl;

  KeyId get keyId;
  @JsonKey(ignore: true)
  _$$CoordinatorToUserKeyGenMessage_FinishedKeyImplCopyWith<
          _$CoordinatorToUserKeyGenMessage_FinishedKeyImpl>
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
abstract class _$$PortEvent_OpenImplCopyWith<$Res> {
  factory _$$PortEvent_OpenImplCopyWith(_$PortEvent_OpenImpl value,
          $Res Function(_$PortEvent_OpenImpl) then) =
      __$$PortEvent_OpenImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PortOpen request});
}

/// @nodoc
class __$$PortEvent_OpenImplCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_OpenImpl>
    implements _$$PortEvent_OpenImplCopyWith<$Res> {
  __$$PortEvent_OpenImplCopyWithImpl(
      _$PortEvent_OpenImpl _value, $Res Function(_$PortEvent_OpenImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_OpenImpl(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortOpen,
    ));
  }
}

/// @nodoc

class _$PortEvent_OpenImpl implements PortEvent_Open {
  const _$PortEvent_OpenImpl({required this.request});

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
            other is _$PortEvent_OpenImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_OpenImplCopyWith<_$PortEvent_OpenImpl> get copyWith =>
      __$$PortEvent_OpenImplCopyWithImpl<_$PortEvent_OpenImpl>(
          this, _$identity);

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
      _$PortEvent_OpenImpl;

  @override
  PortOpen get request;
  @JsonKey(ignore: true)
  _$$PortEvent_OpenImplCopyWith<_$PortEvent_OpenImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_WriteImplCopyWith<$Res> {
  factory _$$PortEvent_WriteImplCopyWith(_$PortEvent_WriteImpl value,
          $Res Function(_$PortEvent_WriteImpl) then) =
      __$$PortEvent_WriteImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PortWrite request});
}

/// @nodoc
class __$$PortEvent_WriteImplCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_WriteImpl>
    implements _$$PortEvent_WriteImplCopyWith<$Res> {
  __$$PortEvent_WriteImplCopyWithImpl(
      _$PortEvent_WriteImpl _value, $Res Function(_$PortEvent_WriteImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_WriteImpl(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortWrite,
    ));
  }
}

/// @nodoc

class _$PortEvent_WriteImpl implements PortEvent_Write {
  const _$PortEvent_WriteImpl({required this.request});

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
            other is _$PortEvent_WriteImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_WriteImplCopyWith<_$PortEvent_WriteImpl> get copyWith =>
      __$$PortEvent_WriteImplCopyWithImpl<_$PortEvent_WriteImpl>(
          this, _$identity);

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
      _$PortEvent_WriteImpl;

  @override
  PortWrite get request;
  @JsonKey(ignore: true)
  _$$PortEvent_WriteImplCopyWith<_$PortEvent_WriteImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_ReadImplCopyWith<$Res> {
  factory _$$PortEvent_ReadImplCopyWith(_$PortEvent_ReadImpl value,
          $Res Function(_$PortEvent_ReadImpl) then) =
      __$$PortEvent_ReadImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PortRead request});
}

/// @nodoc
class __$$PortEvent_ReadImplCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_ReadImpl>
    implements _$$PortEvent_ReadImplCopyWith<$Res> {
  __$$PortEvent_ReadImplCopyWithImpl(
      _$PortEvent_ReadImpl _value, $Res Function(_$PortEvent_ReadImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_ReadImpl(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortRead,
    ));
  }
}

/// @nodoc

class _$PortEvent_ReadImpl implements PortEvent_Read {
  const _$PortEvent_ReadImpl({required this.request});

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
            other is _$PortEvent_ReadImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_ReadImplCopyWith<_$PortEvent_ReadImpl> get copyWith =>
      __$$PortEvent_ReadImplCopyWithImpl<_$PortEvent_ReadImpl>(
          this, _$identity);

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
      _$PortEvent_ReadImpl;

  @override
  PortRead get request;
  @JsonKey(ignore: true)
  _$$PortEvent_ReadImplCopyWith<_$PortEvent_ReadImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$PortEvent_BytesToReadImplCopyWith<$Res> {
  factory _$$PortEvent_BytesToReadImplCopyWith(
          _$PortEvent_BytesToReadImpl value,
          $Res Function(_$PortEvent_BytesToReadImpl) then) =
      __$$PortEvent_BytesToReadImplCopyWithImpl<$Res>;
  @useResult
  $Res call({PortBytesToRead request});
}

/// @nodoc
class __$$PortEvent_BytesToReadImplCopyWithImpl<$Res>
    extends _$PortEventCopyWithImpl<$Res, _$PortEvent_BytesToReadImpl>
    implements _$$PortEvent_BytesToReadImplCopyWith<$Res> {
  __$$PortEvent_BytesToReadImplCopyWithImpl(_$PortEvent_BytesToReadImpl _value,
      $Res Function(_$PortEvent_BytesToReadImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$PortEvent_BytesToReadImpl(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortBytesToRead,
    ));
  }
}

/// @nodoc

class _$PortEvent_BytesToReadImpl implements PortEvent_BytesToRead {
  const _$PortEvent_BytesToReadImpl({required this.request});

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
            other is _$PortEvent_BytesToReadImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$PortEvent_BytesToReadImplCopyWith<_$PortEvent_BytesToReadImpl>
      get copyWith => __$$PortEvent_BytesToReadImplCopyWithImpl<
          _$PortEvent_BytesToReadImpl>(this, _$identity);

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
      {required final PortBytesToRead request}) = _$PortEvent_BytesToReadImpl;

  @override
  PortBytesToRead get request;
  @JsonKey(ignore: true)
  _$$PortEvent_BytesToReadImplCopyWith<_$PortEvent_BytesToReadImpl>
      get copyWith => throw _privateConstructorUsedError;
}
