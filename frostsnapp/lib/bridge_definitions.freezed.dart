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
    required TResult Function(DeviceId field0) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId field0) keyGenAck,
    required TResult Function() finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId field0)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId field0)? keyGenAck,
    TResult? Function()? finishedKey,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId field0)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId field0)? keyGenAck,
    TResult Function()? finishedKey,
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
  $Res call({DeviceId field0});
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
    Object? field0 = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_ReceivedShares(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_ReceivedShares
    implements CoordinatorToUserKeyGenMessage_ReceivedShares {
  const _$CoordinatorToUserKeyGenMessage_ReceivedShares(this.field0);

  @override
  final DeviceId field0;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.receivedShares(field0: $field0)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_ReceivedShares &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

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
    required TResult Function(DeviceId field0) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId field0) keyGenAck,
    required TResult Function() finishedKey,
  }) {
    return receivedShares(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId field0)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId field0)? keyGenAck,
    TResult? Function()? finishedKey,
  }) {
    return receivedShares?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId field0)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId field0)? keyGenAck,
    TResult Function()? finishedKey,
    required TResult orElse(),
  }) {
    if (receivedShares != null) {
      return receivedShares(field0);
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
      final DeviceId field0) = _$CoordinatorToUserKeyGenMessage_ReceivedShares;

  DeviceId get field0;
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
    required TResult Function(DeviceId field0) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId field0) keyGenAck,
    required TResult Function() finishedKey,
  }) {
    return checkKeyGen(sessionHash);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId field0)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId field0)? keyGenAck,
    TResult? Function()? finishedKey,
  }) {
    return checkKeyGen?.call(sessionHash);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId field0)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId field0)? keyGenAck,
    TResult Function()? finishedKey,
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
  $Res call({DeviceId field0});
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
    Object? field0 = null,
  }) {
    return _then(_$CoordinatorToUserKeyGenMessage_KeyGenAck(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_KeyGenAck
    implements CoordinatorToUserKeyGenMessage_KeyGenAck {
  const _$CoordinatorToUserKeyGenMessage_KeyGenAck(this.field0);

  @override
  final DeviceId field0;

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.keyGenAck(field0: $field0)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_KeyGenAck &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

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
    required TResult Function(DeviceId field0) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId field0) keyGenAck,
    required TResult Function() finishedKey,
  }) {
    return keyGenAck(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId field0)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId field0)? keyGenAck,
    TResult? Function()? finishedKey,
  }) {
    return keyGenAck?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId field0)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId field0)? keyGenAck,
    TResult Function()? finishedKey,
    required TResult orElse(),
  }) {
    if (keyGenAck != null) {
      return keyGenAck(field0);
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
      final DeviceId field0) = _$CoordinatorToUserKeyGenMessage_KeyGenAck;

  DeviceId get field0;
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
}

/// @nodoc

class _$CoordinatorToUserKeyGenMessage_FinishedKey
    implements CoordinatorToUserKeyGenMessage_FinishedKey {
  const _$CoordinatorToUserKeyGenMessage_FinishedKey();

  @override
  String toString() {
    return 'CoordinatorToUserKeyGenMessage.finishedKey()';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorToUserKeyGenMessage_FinishedKey);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId field0) receivedShares,
    required TResult Function(U8Array32 sessionHash) checkKeyGen,
    required TResult Function(DeviceId field0) keyGenAck,
    required TResult Function() finishedKey,
  }) {
    return finishedKey();
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId field0)? receivedShares,
    TResult? Function(U8Array32 sessionHash)? checkKeyGen,
    TResult? Function(DeviceId field0)? keyGenAck,
    TResult? Function()? finishedKey,
  }) {
    return finishedKey?.call();
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId field0)? receivedShares,
    TResult Function(U8Array32 sessionHash)? checkKeyGen,
    TResult Function(DeviceId field0)? keyGenAck,
    TResult Function()? finishedKey,
    required TResult orElse(),
  }) {
    if (finishedKey != null) {
      return finishedKey();
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
  const factory CoordinatorToUserKeyGenMessage_FinishedKey() =
      _$CoordinatorToUserKeyGenMessage_FinishedKey;
}

/// @nodoc
mixin _$DeviceChange {
  DeviceId get id => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;

  @JsonKey(ignore: true)
  $DeviceChangeCopyWith<DeviceChange> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $DeviceChangeCopyWith<$Res> {
  factory $DeviceChangeCopyWith(
          DeviceChange value, $Res Function(DeviceChange) then) =
      _$DeviceChangeCopyWithImpl<$Res, DeviceChange>;
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class _$DeviceChangeCopyWithImpl<$Res, $Val extends DeviceChange>
    implements $DeviceChangeCopyWith<$Res> {
  _$DeviceChangeCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_value.copyWith(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ) as $Val);
  }
}

/// @nodoc
abstract class _$$DeviceChange_AddedCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_AddedCopyWith(_$DeviceChange_Added value,
          $Res Function(_$DeviceChange_Added) then) =
      __$$DeviceChange_AddedCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_AddedCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_Added>
    implements _$$DeviceChange_AddedCopyWith<$Res> {
  __$$DeviceChange_AddedCopyWithImpl(
      _$DeviceChange_Added _value, $Res Function(_$DeviceChange_Added) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_Added(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Added implements DeviceChange_Added {
  const _$DeviceChange_Added({required this.id});

  @override
  final DeviceId id;

  @override
  String toString() {
    return 'DeviceChange.added(id: $id)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_Added &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_AddedCopyWith<_$DeviceChange_Added> get copyWith =>
      __$$DeviceChange_AddedCopyWithImpl<_$DeviceChange_Added>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) {
    return added(id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) {
    return added?.call(id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) {
    if (added != null) {
      return added(id);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return added(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return added?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    if (added != null) {
      return added(this);
    }
    return orElse();
  }
}

abstract class DeviceChange_Added implements DeviceChange {
  const factory DeviceChange_Added({required final DeviceId id}) =
      _$DeviceChange_Added;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_AddedCopyWith<_$DeviceChange_Added> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_RenamedCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_RenamedCopyWith(_$DeviceChange_Renamed value,
          $Res Function(_$DeviceChange_Renamed) then) =
      __$$DeviceChange_RenamedCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id, String oldName, String newName});
}

/// @nodoc
class __$$DeviceChange_RenamedCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_Renamed>
    implements _$$DeviceChange_RenamedCopyWith<$Res> {
  __$$DeviceChange_RenamedCopyWithImpl(_$DeviceChange_Renamed _value,
      $Res Function(_$DeviceChange_Renamed) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
    Object? oldName = null,
    Object? newName = null,
  }) {
    return _then(_$DeviceChange_Renamed(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
      oldName: null == oldName
          ? _value.oldName
          : oldName // ignore: cast_nullable_to_non_nullable
              as String,
      newName: null == newName
          ? _value.newName
          : newName // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Renamed implements DeviceChange_Renamed {
  const _$DeviceChange_Renamed(
      {required this.id, required this.oldName, required this.newName});

  @override
  final DeviceId id;
  @override
  final String oldName;
  @override
  final String newName;

  @override
  String toString() {
    return 'DeviceChange.renamed(id: $id, oldName: $oldName, newName: $newName)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_Renamed &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.oldName, oldName) || other.oldName == oldName) &&
            (identical(other.newName, newName) || other.newName == newName));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id, oldName, newName);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_RenamedCopyWith<_$DeviceChange_Renamed> get copyWith =>
      __$$DeviceChange_RenamedCopyWithImpl<_$DeviceChange_Renamed>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) {
    return renamed(id, oldName, newName);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) {
    return renamed?.call(id, oldName, newName);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) {
    if (renamed != null) {
      return renamed(id, oldName, newName);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return renamed(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return renamed?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    if (renamed != null) {
      return renamed(this);
    }
    return orElse();
  }
}

abstract class DeviceChange_Renamed implements DeviceChange {
  const factory DeviceChange_Renamed(
      {required final DeviceId id,
      required final String oldName,
      required final String newName}) = _$DeviceChange_Renamed;

  @override
  DeviceId get id;
  String get oldName;
  String get newName;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_RenamedCopyWith<_$DeviceChange_Renamed> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_NeedsNameCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_NeedsNameCopyWith(_$DeviceChange_NeedsName value,
          $Res Function(_$DeviceChange_NeedsName) then) =
      __$$DeviceChange_NeedsNameCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_NeedsNameCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_NeedsName>
    implements _$$DeviceChange_NeedsNameCopyWith<$Res> {
  __$$DeviceChange_NeedsNameCopyWithImpl(_$DeviceChange_NeedsName _value,
      $Res Function(_$DeviceChange_NeedsName) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_NeedsName(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_NeedsName implements DeviceChange_NeedsName {
  const _$DeviceChange_NeedsName({required this.id});

  @override
  final DeviceId id;

  @override
  String toString() {
    return 'DeviceChange.needsName(id: $id)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_NeedsName &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_NeedsNameCopyWith<_$DeviceChange_NeedsName> get copyWith =>
      __$$DeviceChange_NeedsNameCopyWithImpl<_$DeviceChange_NeedsName>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) {
    return needsName(id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) {
    return needsName?.call(id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) {
    if (needsName != null) {
      return needsName(id);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return needsName(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return needsName?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    if (needsName != null) {
      return needsName(this);
    }
    return orElse();
  }
}

abstract class DeviceChange_NeedsName implements DeviceChange {
  const factory DeviceChange_NeedsName({required final DeviceId id}) =
      _$DeviceChange_NeedsName;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_NeedsNameCopyWith<_$DeviceChange_NeedsName> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_RegisteredCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_RegisteredCopyWith(_$DeviceChange_Registered value,
          $Res Function(_$DeviceChange_Registered) then) =
      __$$DeviceChange_RegisteredCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id, String name});
}

/// @nodoc
class __$$DeviceChange_RegisteredCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_Registered>
    implements _$$DeviceChange_RegisteredCopyWith<$Res> {
  __$$DeviceChange_RegisteredCopyWithImpl(_$DeviceChange_Registered _value,
      $Res Function(_$DeviceChange_Registered) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
    Object? name = null,
  }) {
    return _then(_$DeviceChange_Registered(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
      name: null == name
          ? _value.name
          : name // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Registered implements DeviceChange_Registered {
  const _$DeviceChange_Registered({required this.id, required this.name});

  @override
  final DeviceId id;
  @override
  final String name;

  @override
  String toString() {
    return 'DeviceChange.registered(id: $id, name: $name)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_Registered &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.name, name) || other.name == name));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id, name);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_RegisteredCopyWith<_$DeviceChange_Registered> get copyWith =>
      __$$DeviceChange_RegisteredCopyWithImpl<_$DeviceChange_Registered>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) {
    return registered(id, name);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) {
    return registered?.call(id, name);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) {
    if (registered != null) {
      return registered(id, name);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return registered(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return registered?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    if (registered != null) {
      return registered(this);
    }
    return orElse();
  }
}

abstract class DeviceChange_Registered implements DeviceChange {
  const factory DeviceChange_Registered(
      {required final DeviceId id,
      required final String name}) = _$DeviceChange_Registered;

  @override
  DeviceId get id;
  String get name;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_RegisteredCopyWith<_$DeviceChange_Registered> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_DisconnectedCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_DisconnectedCopyWith(
          _$DeviceChange_Disconnected value,
          $Res Function(_$DeviceChange_Disconnected) then) =
      __$$DeviceChange_DisconnectedCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_DisconnectedCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_Disconnected>
    implements _$$DeviceChange_DisconnectedCopyWith<$Res> {
  __$$DeviceChange_DisconnectedCopyWithImpl(_$DeviceChange_Disconnected _value,
      $Res Function(_$DeviceChange_Disconnected) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_Disconnected(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Disconnected implements DeviceChange_Disconnected {
  const _$DeviceChange_Disconnected({required this.id});

  @override
  final DeviceId id;

  @override
  String toString() {
    return 'DeviceChange.disconnected(id: $id)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_Disconnected &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_DisconnectedCopyWith<_$DeviceChange_Disconnected>
      get copyWith => __$$DeviceChange_DisconnectedCopyWithImpl<
          _$DeviceChange_Disconnected>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DeviceId id) added,
    required TResult Function(DeviceId id, String oldName, String newName)
        renamed,
    required TResult Function(DeviceId id) needsName,
    required TResult Function(DeviceId id, String name) registered,
    required TResult Function(DeviceId id) disconnected,
  }) {
    return disconnected(id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DeviceId id)? added,
    TResult? Function(DeviceId id, String oldName, String newName)? renamed,
    TResult? Function(DeviceId id)? needsName,
    TResult? Function(DeviceId id, String name)? registered,
    TResult? Function(DeviceId id)? disconnected,
  }) {
    return disconnected?.call(id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DeviceId id)? added,
    TResult Function(DeviceId id, String oldName, String newName)? renamed,
    TResult Function(DeviceId id)? needsName,
    TResult Function(DeviceId id, String name)? registered,
    TResult Function(DeviceId id)? disconnected,
    required TResult orElse(),
  }) {
    if (disconnected != null) {
      return disconnected(id);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Renamed value) renamed,
    required TResult Function(DeviceChange_NeedsName value) needsName,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return disconnected(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Renamed value)? renamed,
    TResult? Function(DeviceChange_NeedsName value)? needsName,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return disconnected?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
    TResult Function(DeviceChange_Renamed value)? renamed,
    TResult Function(DeviceChange_NeedsName value)? needsName,
    TResult Function(DeviceChange_Registered value)? registered,
    TResult Function(DeviceChange_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    if (disconnected != null) {
      return disconnected(this);
    }
    return orElse();
  }
}

abstract class DeviceChange_Disconnected implements DeviceChange {
  const factory DeviceChange_Disconnected({required final DeviceId id}) =
      _$DeviceChange_Disconnected;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_DisconnectedCopyWith<_$DeviceChange_Disconnected>
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
