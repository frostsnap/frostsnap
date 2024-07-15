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
}

/// @nodoc
abstract class _$$QrDecoderStatus_ProgressCopyWith<$Res> {
  factory _$$QrDecoderStatus_ProgressCopyWith(_$QrDecoderStatus_Progress value,
          $Res Function(_$QrDecoderStatus_Progress) then) =
      __$$QrDecoderStatus_ProgressCopyWithImpl<$Res>;
  @useResult
  $Res call({DecodingProgress field0});
}

/// @nodoc
class __$$QrDecoderStatus_ProgressCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_Progress>
    implements _$$QrDecoderStatus_ProgressCopyWith<$Res> {
  __$$QrDecoderStatus_ProgressCopyWithImpl(_$QrDecoderStatus_Progress _value,
      $Res Function(_$QrDecoderStatus_Progress) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_Progress(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as DecodingProgress,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_Progress implements QrDecoderStatus_Progress {
  const _$QrDecoderStatus_Progress(this.field0);

  @override
  final DecodingProgress field0;

  @override
  String toString() {
    return 'QrDecoderStatus.progress(field0: $field0)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_Progress &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_ProgressCopyWith<_$QrDecoderStatus_Progress>
      get copyWith =>
          __$$QrDecoderStatus_ProgressCopyWithImpl<_$QrDecoderStatus_Progress>(
              this, _$identity);

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
      _$QrDecoderStatus_Progress;

  @override
  DecodingProgress get field0;
  @JsonKey(ignore: true)
  _$$QrDecoderStatus_ProgressCopyWith<_$QrDecoderStatus_Progress>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$QrDecoderStatus_DecodedCopyWith<$Res> {
  factory _$$QrDecoderStatus_DecodedCopyWith(_$QrDecoderStatus_Decoded value,
          $Res Function(_$QrDecoderStatus_Decoded) then) =
      __$$QrDecoderStatus_DecodedCopyWithImpl<$Res>;
  @useResult
  $Res call({Uint8List field0});
}

/// @nodoc
class __$$QrDecoderStatus_DecodedCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_Decoded>
    implements _$$QrDecoderStatus_DecodedCopyWith<$Res> {
  __$$QrDecoderStatus_DecodedCopyWithImpl(_$QrDecoderStatus_Decoded _value,
      $Res Function(_$QrDecoderStatus_Decoded) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_Decoded(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as Uint8List,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_Decoded implements QrDecoderStatus_Decoded {
  const _$QrDecoderStatus_Decoded(this.field0);

  @override
  final Uint8List field0;

  @override
  String toString() {
    return 'QrDecoderStatus.decoded(field0: $field0)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_Decoded &&
            const DeepCollectionEquality().equals(other.field0, field0));
  }

  @override
  int get hashCode =>
      Object.hash(runtimeType, const DeepCollectionEquality().hash(field0));

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_DecodedCopyWith<_$QrDecoderStatus_Decoded> get copyWith =>
      __$$QrDecoderStatus_DecodedCopyWithImpl<_$QrDecoderStatus_Decoded>(
          this, _$identity);

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
      _$QrDecoderStatus_Decoded;

  @override
  Uint8List get field0;
  @JsonKey(ignore: true)
  _$$QrDecoderStatus_DecodedCopyWith<_$QrDecoderStatus_Decoded> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$QrDecoderStatus_FailedCopyWith<$Res> {
  factory _$$QrDecoderStatus_FailedCopyWith(_$QrDecoderStatus_Failed value,
          $Res Function(_$QrDecoderStatus_Failed) then) =
      __$$QrDecoderStatus_FailedCopyWithImpl<$Res>;
  @useResult
  $Res call({String field0});
}

/// @nodoc
class __$$QrDecoderStatus_FailedCopyWithImpl<$Res>
    extends _$QrDecoderStatusCopyWithImpl<$Res, _$QrDecoderStatus_Failed>
    implements _$$QrDecoderStatus_FailedCopyWith<$Res> {
  __$$QrDecoderStatus_FailedCopyWithImpl(_$QrDecoderStatus_Failed _value,
      $Res Function(_$QrDecoderStatus_Failed) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$QrDecoderStatus_Failed(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$QrDecoderStatus_Failed implements QrDecoderStatus_Failed {
  const _$QrDecoderStatus_Failed(this.field0);

  @override
  final String field0;

  @override
  String toString() {
    return 'QrDecoderStatus.failed(field0: $field0)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$QrDecoderStatus_Failed &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$QrDecoderStatus_FailedCopyWith<_$QrDecoderStatus_Failed> get copyWith =>
      __$$QrDecoderStatus_FailedCopyWithImpl<_$QrDecoderStatus_Failed>(
          this, _$identity);

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
      _$QrDecoderStatus_Failed;

  @override
  String get field0;
  @JsonKey(ignore: true)
  _$$QrDecoderStatus_FailedCopyWith<_$QrDecoderStatus_Failed> get copyWith =>
      throw _privateConstructorUsedError;
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
      _$SignTaskDescription_Plain;

  String get message;
  @JsonKey(ignore: true)
  _$$SignTaskDescription_PlainCopyWith<_$SignTaskDescription_Plain>
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
      _$SignTaskDescription_Transaction;

  UnsignedTx get unsignedTx;
  @JsonKey(ignore: true)
  _$$SignTaskDescription_TransactionCopyWith<_$SignTaskDescription_Transaction>
      get copyWith => throw _privateConstructorUsedError;
}
