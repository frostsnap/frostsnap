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
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#adding-getters-and-methods-to-our-models');

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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
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
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_OpenImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
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
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_WriteImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
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
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_ReadImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
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
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$PortEvent_BytesToReadImpl &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
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

  /// Create a copy of PortEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$PortEvent_BytesToReadImplCopyWith<_$PortEvent_BytesToReadImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$QrDecoderStatus {
  Object get field0 => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DecodingProgress field0) progress,
    required TResult Function(Uint8List field0) decoded,
    required TResult Function(String field0) failed,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DecodingProgress field0)? progress,
    TResult? Function(Uint8List field0)? decoded,
    TResult? Function(String field0)? failed,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DecodingProgress field0)? progress,
    TResult Function(Uint8List field0)? decoded,
    TResult Function(String field0)? failed,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(QrDecoderStatus_Progress value) progress,
    required TResult Function(QrDecoderStatus_Decoded value) decoded,
    required TResult Function(QrDecoderStatus_Failed value) failed,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(QrDecoderStatus_Progress value)? progress,
    TResult? Function(QrDecoderStatus_Decoded value)? decoded,
    TResult? Function(QrDecoderStatus_Failed value)? failed,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(QrDecoderStatus_Progress value)? progress,
    TResult Function(QrDecoderStatus_Decoded value)? decoded,
    TResult Function(QrDecoderStatus_Failed value)? failed,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $QrDecoderStatusCopyWith<$Res> {
  factory $QrDecoderStatusCopyWith(
          QrDecoderStatus value, $Res Function(QrDecoderStatus) then) =
      _$QrDecoderStatusCopyWithImpl<$Res, QrDecoderStatus>;
}

/// @nodoc
class _$QrDecoderStatusCopyWithImpl<$Res, $Val extends QrDecoderStatus>
    implements $QrDecoderStatusCopyWith<$Res> {
  _$QrDecoderStatusCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$QrDecoderStatus_ProgressImplCopyWith<$Res> {
  factory _$$QrDecoderStatus_ProgressImplCopyWith(
          _$QrDecoderStatus_ProgressImpl value,
          $Res Function(_$QrDecoderStatus_ProgressImpl) then) =
      __$$QrDecoderStatus_ProgressImplCopyWithImpl<$Res>;
  @useResult
  $Res call({DecodingProgress field0});
}

/// @nodoc
class __$$QrDecoderStatus_ProgressImplCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_ProgressImpl>
    implements _$$QrDecoderStatus_ProgressImplCopyWith<$Res> {
  __$$QrDecoderStatus_ProgressImplCopyWithImpl(
      _$QrDecoderStatus_ProgressImpl _value,
      $Res Function(_$QrDecoderStatus_ProgressImpl) _then)
      : super(_value, _then);

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_ProgressImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as DecodingProgress,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_ProgressImpl implements QrDecoderStatus_Progress {
  const _$QrDecoderStatus_ProgressImpl(this.field0);

  @override
  final DecodingProgress field0;

  @override
  String toString() {
    return 'QrDecoderStatus.progress(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_ProgressImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_ProgressImplCopyWith<_$QrDecoderStatus_ProgressImpl>
      get copyWith => __$$QrDecoderStatus_ProgressImplCopyWithImpl<
          _$QrDecoderStatus_ProgressImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DecodingProgress field0) progress,
    required TResult Function(Uint8List field0) decoded,
    required TResult Function(String field0) failed,
  }) {
    return progress(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DecodingProgress field0)? progress,
    TResult? Function(Uint8List field0)? decoded,
    TResult? Function(String field0)? failed,
  }) {
    return progress?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DecodingProgress field0)? progress,
    TResult Function(Uint8List field0)? decoded,
    TResult Function(String field0)? failed,
    required TResult orElse(),
  }) {
    if (progress != null) {
      return progress(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(QrDecoderStatus_Progress value) progress,
    required TResult Function(QrDecoderStatus_Decoded value) decoded,
    required TResult Function(QrDecoderStatus_Failed value) failed,
  }) {
    return progress(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(QrDecoderStatus_Progress value)? progress,
    TResult? Function(QrDecoderStatus_Decoded value)? decoded,
    TResult? Function(QrDecoderStatus_Failed value)? failed,
  }) {
    return progress?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(QrDecoderStatus_Progress value)? progress,
    TResult Function(QrDecoderStatus_Decoded value)? decoded,
    TResult Function(QrDecoderStatus_Failed value)? failed,
    required TResult orElse(),
  }) {
    if (progress != null) {
      return progress(this);
    }
    return orElse();
  }
}

abstract class QrDecoderStatus_Progress implements QrDecoderStatus {
  const factory QrDecoderStatus_Progress(final DecodingProgress field0) =
      _$QrDecoderStatus_ProgressImpl;

  @override
  DecodingProgress get field0;

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$QrDecoderStatus_ProgressImplCopyWith<_$QrDecoderStatus_ProgressImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$QrDecoderStatus_DecodedImplCopyWith<$Res> {
  factory _$$QrDecoderStatus_DecodedImplCopyWith(
          _$QrDecoderStatus_DecodedImpl value,
          $Res Function(_$QrDecoderStatus_DecodedImpl) then) =
      __$$QrDecoderStatus_DecodedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({Uint8List field0});
}

/// @nodoc
class __$$QrDecoderStatus_DecodedImplCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_DecodedImpl>
    implements _$$QrDecoderStatus_DecodedImplCopyWith<$Res> {
  __$$QrDecoderStatus_DecodedImplCopyWithImpl(
      _$QrDecoderStatus_DecodedImpl _value,
      $Res Function(_$QrDecoderStatus_DecodedImpl) _then)
      : super(_value, _then);

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_DecodedImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as Uint8List,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_DecodedImpl implements QrDecoderStatus_Decoded {
  const _$QrDecoderStatus_DecodedImpl(this.field0);

  @override
  final Uint8List field0;

  @override
  String toString() {
    return 'QrDecoderStatus.decoded(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_DecodedImpl &&
            const DeepCollectionEquality().equals(other.field0, field0));
  }

  @override
  int get hashCode =>
      Object.hash(runtimeType, const DeepCollectionEquality().hash(field0));

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_DecodedImplCopyWith<_$QrDecoderStatus_DecodedImpl>
      get copyWith => __$$QrDecoderStatus_DecodedImplCopyWithImpl<
          _$QrDecoderStatus_DecodedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DecodingProgress field0) progress,
    required TResult Function(Uint8List field0) decoded,
    required TResult Function(String field0) failed,
  }) {
    return decoded(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DecodingProgress field0)? progress,
    TResult? Function(Uint8List field0)? decoded,
    TResult? Function(String field0)? failed,
  }) {
    return decoded?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DecodingProgress field0)? progress,
    TResult Function(Uint8List field0)? decoded,
    TResult Function(String field0)? failed,
    required TResult orElse(),
  }) {
    if (decoded != null) {
      return decoded(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(QrDecoderStatus_Progress value) progress,
    required TResult Function(QrDecoderStatus_Decoded value) decoded,
    required TResult Function(QrDecoderStatus_Failed value) failed,
  }) {
    return decoded(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(QrDecoderStatus_Progress value)? progress,
    TResult? Function(QrDecoderStatus_Decoded value)? decoded,
    TResult? Function(QrDecoderStatus_Failed value)? failed,
  }) {
    return decoded?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(QrDecoderStatus_Progress value)? progress,
    TResult Function(QrDecoderStatus_Decoded value)? decoded,
    TResult Function(QrDecoderStatus_Failed value)? failed,
    required TResult orElse(),
  }) {
    if (decoded != null) {
      return decoded(this);
    }
    return orElse();
  }
}

abstract class QrDecoderStatus_Decoded implements QrDecoderStatus {
  const factory QrDecoderStatus_Decoded(final Uint8List field0) =
      _$QrDecoderStatus_DecodedImpl;

  @override
  Uint8List get field0;

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$QrDecoderStatus_DecodedImplCopyWith<_$QrDecoderStatus_DecodedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$QrDecoderStatus_FailedImplCopyWith<$Res> {
  factory _$$QrDecoderStatus_FailedImplCopyWith(
          _$QrDecoderStatus_FailedImpl value,
          $Res Function(_$QrDecoderStatus_FailedImpl) then) =
      __$$QrDecoderStatus_FailedImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String field0});
}

/// @nodoc
class __$$QrDecoderStatus_FailedImplCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_FailedImpl>
    implements _$$QrDecoderStatus_FailedImplCopyWith<$Res> {
  __$$QrDecoderStatus_FailedImplCopyWithImpl(
      _$QrDecoderStatus_FailedImpl _value,
      $Res Function(_$QrDecoderStatus_FailedImpl) _then)
      : super(_value, _then);

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_FailedImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_FailedImpl implements QrDecoderStatus_Failed {
  const _$QrDecoderStatus_FailedImpl(this.field0);

  @override
  final String field0;

  @override
  String toString() {
    return 'QrDecoderStatus.failed(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_FailedImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_FailedImplCopyWith<_$QrDecoderStatus_FailedImpl>
      get copyWith => __$$QrDecoderStatus_FailedImplCopyWithImpl<
          _$QrDecoderStatus_FailedImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(DecodingProgress field0) progress,
    required TResult Function(Uint8List field0) decoded,
    required TResult Function(String field0) failed,
  }) {
    return failed(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(DecodingProgress field0)? progress,
    TResult? Function(Uint8List field0)? decoded,
    TResult? Function(String field0)? failed,
  }) {
    return failed?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(DecodingProgress field0)? progress,
    TResult Function(Uint8List field0)? decoded,
    TResult Function(String field0)? failed,
    required TResult orElse(),
  }) {
    if (failed != null) {
      return failed(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(QrDecoderStatus_Progress value) progress,
    required TResult Function(QrDecoderStatus_Decoded value) decoded,
    required TResult Function(QrDecoderStatus_Failed value) failed,
  }) {
    return failed(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(QrDecoderStatus_Progress value)? progress,
    TResult? Function(QrDecoderStatus_Decoded value)? decoded,
    TResult? Function(QrDecoderStatus_Failed value)? failed,
  }) {
    return failed?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(QrDecoderStatus_Progress value)? progress,
    TResult Function(QrDecoderStatus_Decoded value)? decoded,
    TResult Function(QrDecoderStatus_Failed value)? failed,
    required TResult orElse(),
  }) {
    if (failed != null) {
      return failed(this);
    }
    return orElse();
  }
}

abstract class QrDecoderStatus_Failed implements QrDecoderStatus {
  const factory QrDecoderStatus_Failed(final String field0) =
      _$QrDecoderStatus_FailedImpl;

  @override
  String get field0;

  /// Create a copy of QrDecoderStatus
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$QrDecoderStatus_FailedImplCopyWith<_$QrDecoderStatus_FailedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$SignTaskDescription {
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
    TResult Function(UnsignedTx unsignedTx)? transaction,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(SignTaskDescription_Plain value) plain,
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
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

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
}

/// @nodoc
abstract class _$$SignTaskDescription_PlainImplCopyWith<$Res> {
  factory _$$SignTaskDescription_PlainImplCopyWith(
          _$SignTaskDescription_PlainImpl value,
          $Res Function(_$SignTaskDescription_PlainImpl) then) =
      __$$SignTaskDescription_PlainImplCopyWithImpl<$Res>;
  @useResult
  $Res call({String message});
}

/// @nodoc
class __$$SignTaskDescription_PlainImplCopyWithImpl<$Res>
    extends _$SignTaskDescriptionCopyWithImpl<$Res,
        _$SignTaskDescription_PlainImpl>
    implements _$$SignTaskDescription_PlainImplCopyWith<$Res> {
  __$$SignTaskDescription_PlainImplCopyWithImpl(
      _$SignTaskDescription_PlainImpl _value,
      $Res Function(_$SignTaskDescription_PlainImpl) _then)
      : super(_value, _then);

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? message = null,
  }) {
    return _then(_$SignTaskDescription_PlainImpl(
      message: null == message
          ? _value.message
          : message // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$SignTaskDescription_PlainImpl implements SignTaskDescription_Plain {
  const _$SignTaskDescription_PlainImpl({required this.message});

  @override
  final String message;

  @override
  String toString() {
    return 'SignTaskDescription.plain(message: $message)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SignTaskDescription_PlainImpl &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode => Object.hash(runtimeType, message);

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SignTaskDescription_PlainImplCopyWith<_$SignTaskDescription_PlainImpl>
      get copyWith => __$$SignTaskDescription_PlainImplCopyWithImpl<
          _$SignTaskDescription_PlainImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) {
    return plain(message);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) {
    return plain?.call(message);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
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
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) {
    return plain(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) {
    return plain?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
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
      _$SignTaskDescription_PlainImpl;

  String get message;

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SignTaskDescription_PlainImplCopyWith<_$SignTaskDescription_PlainImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$SignTaskDescription_TransactionImplCopyWith<$Res> {
  factory _$$SignTaskDescription_TransactionImplCopyWith(
          _$SignTaskDescription_TransactionImpl value,
          $Res Function(_$SignTaskDescription_TransactionImpl) then) =
      __$$SignTaskDescription_TransactionImplCopyWithImpl<$Res>;
  @useResult
  $Res call({UnsignedTx unsignedTx});
}

/// @nodoc
class __$$SignTaskDescription_TransactionImplCopyWithImpl<$Res>
    extends _$SignTaskDescriptionCopyWithImpl<$Res,
        _$SignTaskDescription_TransactionImpl>
    implements _$$SignTaskDescription_TransactionImplCopyWith<$Res> {
  __$$SignTaskDescription_TransactionImplCopyWithImpl(
      _$SignTaskDescription_TransactionImpl _value,
      $Res Function(_$SignTaskDescription_TransactionImpl) _then)
      : super(_value, _then);

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? unsignedTx = null,
  }) {
    return _then(_$SignTaskDescription_TransactionImpl(
      unsignedTx: null == unsignedTx
          ? _value.unsignedTx
          : unsignedTx // ignore: cast_nullable_to_non_nullable
              as UnsignedTx,
    ));
  }
}

/// @nodoc

class _$SignTaskDescription_TransactionImpl
    implements SignTaskDescription_Transaction {
  const _$SignTaskDescription_TransactionImpl({required this.unsignedTx});

  @override
  final UnsignedTx unsignedTx;

  @override
  String toString() {
    return 'SignTaskDescription.transaction(unsignedTx: $unsignedTx)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$SignTaskDescription_TransactionImpl &&
            (identical(other.unsignedTx, unsignedTx) ||
                other.unsignedTx == unsignedTx));
  }

  @override
  int get hashCode => Object.hash(runtimeType, unsignedTx);

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @override
  @pragma('vm:prefer-inline')
  _$$SignTaskDescription_TransactionImplCopyWith<
          _$SignTaskDescription_TransactionImpl>
      get copyWith => __$$SignTaskDescription_TransactionImplCopyWithImpl<
          _$SignTaskDescription_TransactionImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String message) plain,
    required TResult Function(UnsignedTx unsignedTx) transaction,
  }) {
    return transaction(unsignedTx);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String message)? plain,
    TResult? Function(UnsignedTx unsignedTx)? transaction,
  }) {
    return transaction?.call(unsignedTx);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String message)? plain,
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
    required TResult Function(SignTaskDescription_Transaction value)
        transaction,
  }) {
    return transaction(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(SignTaskDescription_Plain value)? plain,
    TResult? Function(SignTaskDescription_Transaction value)? transaction,
  }) {
    return transaction?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(SignTaskDescription_Plain value)? plain,
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
      _$SignTaskDescription_TransactionImpl;

  UnsignedTx get unsignedTx;

  /// Create a copy of SignTaskDescription
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  _$$SignTaskDescription_TransactionImplCopyWith<
          _$SignTaskDescription_TransactionImpl>
      get copyWith => throw _privateConstructorUsedError;
}
