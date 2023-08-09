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
mixin _$CoordinatorEvent {
  Object get request => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) portOpen,
    required TResult Function(PortWrite request) portWrite,
    required TResult Function(PortRead request) portRead,
    required TResult Function(PortBytesToRead request) portBytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? portOpen,
    TResult? Function(PortWrite request)? portWrite,
    TResult? Function(PortRead request)? portRead,
    TResult? Function(PortBytesToRead request)? portBytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? portOpen,
    TResult Function(PortWrite request)? portWrite,
    TResult Function(PortRead request)? portRead,
    TResult Function(PortBytesToRead request)? portBytesToRead,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CoordinatorEvent_PortOpen value) portOpen,
    required TResult Function(CoordinatorEvent_PortWrite value) portWrite,
    required TResult Function(CoordinatorEvent_PortRead value) portRead,
    required TResult Function(CoordinatorEvent_PortBytesToRead value)
        portBytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult? Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult? Function(CoordinatorEvent_PortRead value)? portRead,
    TResult? Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult Function(CoordinatorEvent_PortRead value)? portRead,
    TResult Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $CoordinatorEventCopyWith<$Res> {
  factory $CoordinatorEventCopyWith(
          CoordinatorEvent value, $Res Function(CoordinatorEvent) then) =
      _$CoordinatorEventCopyWithImpl<$Res, CoordinatorEvent>;
}

/// @nodoc
class _$CoordinatorEventCopyWithImpl<$Res, $Val extends CoordinatorEvent>
    implements $CoordinatorEventCopyWith<$Res> {
  _$CoordinatorEventCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$CoordinatorEvent_PortOpenCopyWith<$Res> {
  factory _$$CoordinatorEvent_PortOpenCopyWith(
          _$CoordinatorEvent_PortOpen value,
          $Res Function(_$CoordinatorEvent_PortOpen) then) =
      __$$CoordinatorEvent_PortOpenCopyWithImpl<$Res>;
  @useResult
  $Res call({PortOpen request});
}

/// @nodoc
class __$$CoordinatorEvent_PortOpenCopyWithImpl<$Res>
    extends _$CoordinatorEventCopyWithImpl<$Res, _$CoordinatorEvent_PortOpen>
    implements _$$CoordinatorEvent_PortOpenCopyWith<$Res> {
  __$$CoordinatorEvent_PortOpenCopyWithImpl(_$CoordinatorEvent_PortOpen _value,
      $Res Function(_$CoordinatorEvent_PortOpen) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$CoordinatorEvent_PortOpen(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortOpen,
    ));
  }
}

/// @nodoc

class _$CoordinatorEvent_PortOpen implements CoordinatorEvent_PortOpen {
  const _$CoordinatorEvent_PortOpen({required this.request});

  @override
  final PortOpen request;

  @override
  String toString() {
    return 'CoordinatorEvent.portOpen(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorEvent_PortOpen &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorEvent_PortOpenCopyWith<_$CoordinatorEvent_PortOpen>
      get copyWith => __$$CoordinatorEvent_PortOpenCopyWithImpl<
          _$CoordinatorEvent_PortOpen>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) portOpen,
    required TResult Function(PortWrite request) portWrite,
    required TResult Function(PortRead request) portRead,
    required TResult Function(PortBytesToRead request) portBytesToRead,
  }) {
    return portOpen(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? portOpen,
    TResult? Function(PortWrite request)? portWrite,
    TResult? Function(PortRead request)? portRead,
    TResult? Function(PortBytesToRead request)? portBytesToRead,
  }) {
    return portOpen?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? portOpen,
    TResult Function(PortWrite request)? portWrite,
    TResult Function(PortRead request)? portRead,
    TResult Function(PortBytesToRead request)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portOpen != null) {
      return portOpen(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CoordinatorEvent_PortOpen value) portOpen,
    required TResult Function(CoordinatorEvent_PortWrite value) portWrite,
    required TResult Function(CoordinatorEvent_PortRead value) portRead,
    required TResult Function(CoordinatorEvent_PortBytesToRead value)
        portBytesToRead,
  }) {
    return portOpen(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult? Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult? Function(CoordinatorEvent_PortRead value)? portRead,
    TResult? Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
  }) {
    return portOpen?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult Function(CoordinatorEvent_PortRead value)? portRead,
    TResult Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portOpen != null) {
      return portOpen(this);
    }
    return orElse();
  }
}

abstract class CoordinatorEvent_PortOpen implements CoordinatorEvent {
  const factory CoordinatorEvent_PortOpen({required final PortOpen request}) =
      _$CoordinatorEvent_PortOpen;

  @override
  PortOpen get request;
  @JsonKey(ignore: true)
  _$$CoordinatorEvent_PortOpenCopyWith<_$CoordinatorEvent_PortOpen>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorEvent_PortWriteCopyWith<$Res> {
  factory _$$CoordinatorEvent_PortWriteCopyWith(
          _$CoordinatorEvent_PortWrite value,
          $Res Function(_$CoordinatorEvent_PortWrite) then) =
      __$$CoordinatorEvent_PortWriteCopyWithImpl<$Res>;
  @useResult
  $Res call({PortWrite request});
}

/// @nodoc
class __$$CoordinatorEvent_PortWriteCopyWithImpl<$Res>
    extends _$CoordinatorEventCopyWithImpl<$Res, _$CoordinatorEvent_PortWrite>
    implements _$$CoordinatorEvent_PortWriteCopyWith<$Res> {
  __$$CoordinatorEvent_PortWriteCopyWithImpl(
      _$CoordinatorEvent_PortWrite _value,
      $Res Function(_$CoordinatorEvent_PortWrite) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$CoordinatorEvent_PortWrite(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortWrite,
    ));
  }
}

/// @nodoc

class _$CoordinatorEvent_PortWrite implements CoordinatorEvent_PortWrite {
  const _$CoordinatorEvent_PortWrite({required this.request});

  @override
  final PortWrite request;

  @override
  String toString() {
    return 'CoordinatorEvent.portWrite(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorEvent_PortWrite &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorEvent_PortWriteCopyWith<_$CoordinatorEvent_PortWrite>
      get copyWith => __$$CoordinatorEvent_PortWriteCopyWithImpl<
          _$CoordinatorEvent_PortWrite>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) portOpen,
    required TResult Function(PortWrite request) portWrite,
    required TResult Function(PortRead request) portRead,
    required TResult Function(PortBytesToRead request) portBytesToRead,
  }) {
    return portWrite(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? portOpen,
    TResult? Function(PortWrite request)? portWrite,
    TResult? Function(PortRead request)? portRead,
    TResult? Function(PortBytesToRead request)? portBytesToRead,
  }) {
    return portWrite?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? portOpen,
    TResult Function(PortWrite request)? portWrite,
    TResult Function(PortRead request)? portRead,
    TResult Function(PortBytesToRead request)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portWrite != null) {
      return portWrite(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CoordinatorEvent_PortOpen value) portOpen,
    required TResult Function(CoordinatorEvent_PortWrite value) portWrite,
    required TResult Function(CoordinatorEvent_PortRead value) portRead,
    required TResult Function(CoordinatorEvent_PortBytesToRead value)
        portBytesToRead,
  }) {
    return portWrite(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult? Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult? Function(CoordinatorEvent_PortRead value)? portRead,
    TResult? Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
  }) {
    return portWrite?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult Function(CoordinatorEvent_PortRead value)? portRead,
    TResult Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portWrite != null) {
      return portWrite(this);
    }
    return orElse();
  }
}

abstract class CoordinatorEvent_PortWrite implements CoordinatorEvent {
  const factory CoordinatorEvent_PortWrite({required final PortWrite request}) =
      _$CoordinatorEvent_PortWrite;

  @override
  PortWrite get request;
  @JsonKey(ignore: true)
  _$$CoordinatorEvent_PortWriteCopyWith<_$CoordinatorEvent_PortWrite>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorEvent_PortReadCopyWith<$Res> {
  factory _$$CoordinatorEvent_PortReadCopyWith(
          _$CoordinatorEvent_PortRead value,
          $Res Function(_$CoordinatorEvent_PortRead) then) =
      __$$CoordinatorEvent_PortReadCopyWithImpl<$Res>;
  @useResult
  $Res call({PortRead request});
}

/// @nodoc
class __$$CoordinatorEvent_PortReadCopyWithImpl<$Res>
    extends _$CoordinatorEventCopyWithImpl<$Res, _$CoordinatorEvent_PortRead>
    implements _$$CoordinatorEvent_PortReadCopyWith<$Res> {
  __$$CoordinatorEvent_PortReadCopyWithImpl(_$CoordinatorEvent_PortRead _value,
      $Res Function(_$CoordinatorEvent_PortRead) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$CoordinatorEvent_PortRead(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortRead,
    ));
  }
}

/// @nodoc

class _$CoordinatorEvent_PortRead implements CoordinatorEvent_PortRead {
  const _$CoordinatorEvent_PortRead({required this.request});

  @override
  final PortRead request;

  @override
  String toString() {
    return 'CoordinatorEvent.portRead(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorEvent_PortRead &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorEvent_PortReadCopyWith<_$CoordinatorEvent_PortRead>
      get copyWith => __$$CoordinatorEvent_PortReadCopyWithImpl<
          _$CoordinatorEvent_PortRead>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) portOpen,
    required TResult Function(PortWrite request) portWrite,
    required TResult Function(PortRead request) portRead,
    required TResult Function(PortBytesToRead request) portBytesToRead,
  }) {
    return portRead(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? portOpen,
    TResult? Function(PortWrite request)? portWrite,
    TResult? Function(PortRead request)? portRead,
    TResult? Function(PortBytesToRead request)? portBytesToRead,
  }) {
    return portRead?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? portOpen,
    TResult Function(PortWrite request)? portWrite,
    TResult Function(PortRead request)? portRead,
    TResult Function(PortBytesToRead request)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portRead != null) {
      return portRead(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CoordinatorEvent_PortOpen value) portOpen,
    required TResult Function(CoordinatorEvent_PortWrite value) portWrite,
    required TResult Function(CoordinatorEvent_PortRead value) portRead,
    required TResult Function(CoordinatorEvent_PortBytesToRead value)
        portBytesToRead,
  }) {
    return portRead(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult? Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult? Function(CoordinatorEvent_PortRead value)? portRead,
    TResult? Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
  }) {
    return portRead?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult Function(CoordinatorEvent_PortRead value)? portRead,
    TResult Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portRead != null) {
      return portRead(this);
    }
    return orElse();
  }
}

abstract class CoordinatorEvent_PortRead implements CoordinatorEvent {
  const factory CoordinatorEvent_PortRead({required final PortRead request}) =
      _$CoordinatorEvent_PortRead;

  @override
  PortRead get request;
  @JsonKey(ignore: true)
  _$$CoordinatorEvent_PortReadCopyWith<_$CoordinatorEvent_PortRead>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CoordinatorEvent_PortBytesToReadCopyWith<$Res> {
  factory _$$CoordinatorEvent_PortBytesToReadCopyWith(
          _$CoordinatorEvent_PortBytesToRead value,
          $Res Function(_$CoordinatorEvent_PortBytesToRead) then) =
      __$$CoordinatorEvent_PortBytesToReadCopyWithImpl<$Res>;
  @useResult
  $Res call({PortBytesToRead request});
}

/// @nodoc
class __$$CoordinatorEvent_PortBytesToReadCopyWithImpl<$Res>
    extends _$CoordinatorEventCopyWithImpl<$Res,
        _$CoordinatorEvent_PortBytesToRead>
    implements _$$CoordinatorEvent_PortBytesToReadCopyWith<$Res> {
  __$$CoordinatorEvent_PortBytesToReadCopyWithImpl(
      _$CoordinatorEvent_PortBytesToRead _value,
      $Res Function(_$CoordinatorEvent_PortBytesToRead) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? request = null,
  }) {
    return _then(_$CoordinatorEvent_PortBytesToRead(
      request: null == request
          ? _value.request
          : request // ignore: cast_nullable_to_non_nullable
              as PortBytesToRead,
    ));
  }
}

/// @nodoc

class _$CoordinatorEvent_PortBytesToRead
    implements CoordinatorEvent_PortBytesToRead {
  const _$CoordinatorEvent_PortBytesToRead({required this.request});

  @override
  final PortBytesToRead request;

  @override
  String toString() {
    return 'CoordinatorEvent.portBytesToRead(request: $request)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CoordinatorEvent_PortBytesToRead &&
            (identical(other.request, request) || other.request == request));
  }

  @override
  int get hashCode => Object.hash(runtimeType, request);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CoordinatorEvent_PortBytesToReadCopyWith<
          _$CoordinatorEvent_PortBytesToRead>
      get copyWith => __$$CoordinatorEvent_PortBytesToReadCopyWithImpl<
          _$CoordinatorEvent_PortBytesToRead>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(PortOpen request) portOpen,
    required TResult Function(PortWrite request) portWrite,
    required TResult Function(PortRead request) portRead,
    required TResult Function(PortBytesToRead request) portBytesToRead,
  }) {
    return portBytesToRead(request);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(PortOpen request)? portOpen,
    TResult? Function(PortWrite request)? portWrite,
    TResult? Function(PortRead request)? portRead,
    TResult? Function(PortBytesToRead request)? portBytesToRead,
  }) {
    return portBytesToRead?.call(request);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(PortOpen request)? portOpen,
    TResult Function(PortWrite request)? portWrite,
    TResult Function(PortRead request)? portRead,
    TResult Function(PortBytesToRead request)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portBytesToRead != null) {
      return portBytesToRead(request);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CoordinatorEvent_PortOpen value) portOpen,
    required TResult Function(CoordinatorEvent_PortWrite value) portWrite,
    required TResult Function(CoordinatorEvent_PortRead value) portRead,
    required TResult Function(CoordinatorEvent_PortBytesToRead value)
        portBytesToRead,
  }) {
    return portBytesToRead(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult? Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult? Function(CoordinatorEvent_PortRead value)? portRead,
    TResult? Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
  }) {
    return portBytesToRead?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CoordinatorEvent_PortOpen value)? portOpen,
    TResult Function(CoordinatorEvent_PortWrite value)? portWrite,
    TResult Function(CoordinatorEvent_PortRead value)? portRead,
    TResult Function(CoordinatorEvent_PortBytesToRead value)? portBytesToRead,
    required TResult orElse(),
  }) {
    if (portBytesToRead != null) {
      return portBytesToRead(this);
    }
    return orElse();
  }
}

abstract class CoordinatorEvent_PortBytesToRead implements CoordinatorEvent {
  const factory CoordinatorEvent_PortBytesToRead(
          {required final PortBytesToRead request}) =
      _$CoordinatorEvent_PortBytesToRead;

  @override
  PortBytesToRead get request;
  @JsonKey(ignore: true)
  _$$CoordinatorEvent_PortBytesToReadCopyWith<
          _$CoordinatorEvent_PortBytesToRead>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$DeviceChange {
  String get id => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String id) added,
    required TResult Function(String id, String label) registered,
    required TResult Function(String id) disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String id)? added,
    TResult? Function(String id, String label)? registered,
    TResult? Function(String id)? disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String id)? added,
    TResult Function(String id, String label)? registered,
    TResult Function(String id)? disconnected,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
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
  $Res call({String id});
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
              as String,
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
  $Res call({String id});
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
              as String,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Added implements DeviceChange_Added {
  const _$DeviceChange_Added({required this.id});

  @override
  final String id;

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
    required TResult Function(String id) added,
    required TResult Function(String id, String label) registered,
    required TResult Function(String id) disconnected,
  }) {
    return added(id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String id)? added,
    TResult? Function(String id, String label)? registered,
    TResult? Function(String id)? disconnected,
  }) {
    return added?.call(id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String id)? added,
    TResult Function(String id, String label)? registered,
    TResult Function(String id)? disconnected,
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
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return added(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return added?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
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
  const factory DeviceChange_Added({required final String id}) =
      _$DeviceChange_Added;

  @override
  String get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_AddedCopyWith<_$DeviceChange_Added> get copyWith =>
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
  $Res call({String id, String label});
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
    Object? label = null,
  }) {
    return _then(_$DeviceChange_Registered(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as String,
      label: null == label
          ? _value.label
          : label // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Registered implements DeviceChange_Registered {
  const _$DeviceChange_Registered({required this.id, required this.label});

  @override
  final String id;
  @override
  final String label;

  @override
  String toString() {
    return 'DeviceChange.registered(id: $id, label: $label)';
  }

  @override
  bool operator ==(dynamic other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$DeviceChange_Registered &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.label, label) || other.label == label));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id, label);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_RegisteredCopyWith<_$DeviceChange_Registered> get copyWith =>
      __$$DeviceChange_RegisteredCopyWithImpl<_$DeviceChange_Registered>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(String id) added,
    required TResult Function(String id, String label) registered,
    required TResult Function(String id) disconnected,
  }) {
    return registered(id, label);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String id)? added,
    TResult? Function(String id, String label)? registered,
    TResult? Function(String id)? disconnected,
  }) {
    return registered?.call(id, label);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String id)? added,
    TResult Function(String id, String label)? registered,
    TResult Function(String id)? disconnected,
    required TResult orElse(),
  }) {
    if (registered != null) {
      return registered(id, label);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(DeviceChange_Added value) added,
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return registered(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return registered?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
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
      {required final String id,
      required final String label}) = _$DeviceChange_Registered;

  @override
  String get id;
  String get label;
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
  $Res call({String id});
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
              as String,
    ));
  }
}

/// @nodoc

class _$DeviceChange_Disconnected implements DeviceChange_Disconnected {
  const _$DeviceChange_Disconnected({required this.id});

  @override
  final String id;

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
    required TResult Function(String id) added,
    required TResult Function(String id, String label) registered,
    required TResult Function(String id) disconnected,
  }) {
    return disconnected(id);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(String id)? added,
    TResult? Function(String id, String label)? registered,
    TResult? Function(String id)? disconnected,
  }) {
    return disconnected?.call(id);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(String id)? added,
    TResult Function(String id, String label)? registered,
    TResult Function(String id)? disconnected,
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
    required TResult Function(DeviceChange_Registered value) registered,
    required TResult Function(DeviceChange_Disconnected value) disconnected,
  }) {
    return disconnected(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(DeviceChange_Added value)? added,
    TResult? Function(DeviceChange_Registered value)? registered,
    TResult? Function(DeviceChange_Disconnected value)? disconnected,
  }) {
    return disconnected?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(DeviceChange_Added value)? added,
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
  const factory DeviceChange_Disconnected({required final String id}) =
      _$DeviceChange_Disconnected;

  @override
  String get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_DisconnectedCopyWith<_$DeviceChange_Disconnected>
      get copyWith => throw _privateConstructorUsedError;
}
