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
