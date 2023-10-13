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
abstract class _$$DeviceChange_AddedImplCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_AddedImplCopyWith(_$DeviceChange_AddedImpl value,
          $Res Function(_$DeviceChange_AddedImpl) then) =
      __$$DeviceChange_AddedImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_AddedImplCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_AddedImpl>
    implements _$$DeviceChange_AddedImplCopyWith<$Res> {
  __$$DeviceChange_AddedImplCopyWithImpl(_$DeviceChange_AddedImpl _value,
      $Res Function(_$DeviceChange_AddedImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_AddedImpl(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_AddedImpl implements DeviceChange_Added {
  const _$DeviceChange_AddedImpl({required this.id});

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
            other is _$DeviceChange_AddedImpl &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_AddedImplCopyWith<_$DeviceChange_AddedImpl> get copyWith =>
      __$$DeviceChange_AddedImplCopyWithImpl<_$DeviceChange_AddedImpl>(
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
      _$DeviceChange_AddedImpl;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_AddedImplCopyWith<_$DeviceChange_AddedImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_RenamedImplCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_RenamedImplCopyWith(_$DeviceChange_RenamedImpl value,
          $Res Function(_$DeviceChange_RenamedImpl) then) =
      __$$DeviceChange_RenamedImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id, String oldName, String newName});
}

/// @nodoc
class __$$DeviceChange_RenamedImplCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_RenamedImpl>
    implements _$$DeviceChange_RenamedImplCopyWith<$Res> {
  __$$DeviceChange_RenamedImplCopyWithImpl(_$DeviceChange_RenamedImpl _value,
      $Res Function(_$DeviceChange_RenamedImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
    Object? oldName = null,
    Object? newName = null,
  }) {
    return _then(_$DeviceChange_RenamedImpl(
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

class _$DeviceChange_RenamedImpl implements DeviceChange_Renamed {
  const _$DeviceChange_RenamedImpl(
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
            other is _$DeviceChange_RenamedImpl &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.oldName, oldName) || other.oldName == oldName) &&
            (identical(other.newName, newName) || other.newName == newName));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id, oldName, newName);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_RenamedImplCopyWith<_$DeviceChange_RenamedImpl>
      get copyWith =>
          __$$DeviceChange_RenamedImplCopyWithImpl<_$DeviceChange_RenamedImpl>(
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
      required final String newName}) = _$DeviceChange_RenamedImpl;

  @override
  DeviceId get id;
  String get oldName;
  String get newName;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_RenamedImplCopyWith<_$DeviceChange_RenamedImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_NeedsNameImplCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_NeedsNameImplCopyWith(
          _$DeviceChange_NeedsNameImpl value,
          $Res Function(_$DeviceChange_NeedsNameImpl) then) =
      __$$DeviceChange_NeedsNameImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_NeedsNameImplCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_NeedsNameImpl>
    implements _$$DeviceChange_NeedsNameImplCopyWith<$Res> {
  __$$DeviceChange_NeedsNameImplCopyWithImpl(
      _$DeviceChange_NeedsNameImpl _value,
      $Res Function(_$DeviceChange_NeedsNameImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_NeedsNameImpl(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_NeedsNameImpl implements DeviceChange_NeedsName {
  const _$DeviceChange_NeedsNameImpl({required this.id});

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
            other is _$DeviceChange_NeedsNameImpl &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_NeedsNameImplCopyWith<_$DeviceChange_NeedsNameImpl>
      get copyWith => __$$DeviceChange_NeedsNameImplCopyWithImpl<
          _$DeviceChange_NeedsNameImpl>(this, _$identity);

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
      _$DeviceChange_NeedsNameImpl;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_NeedsNameImplCopyWith<_$DeviceChange_NeedsNameImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_RegisteredImplCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_RegisteredImplCopyWith(
          _$DeviceChange_RegisteredImpl value,
          $Res Function(_$DeviceChange_RegisteredImpl) then) =
      __$$DeviceChange_RegisteredImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id, String name});
}

/// @nodoc
class __$$DeviceChange_RegisteredImplCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_RegisteredImpl>
    implements _$$DeviceChange_RegisteredImplCopyWith<$Res> {
  __$$DeviceChange_RegisteredImplCopyWithImpl(
      _$DeviceChange_RegisteredImpl _value,
      $Res Function(_$DeviceChange_RegisteredImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
    Object? name = null,
  }) {
    return _then(_$DeviceChange_RegisteredImpl(
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

class _$DeviceChange_RegisteredImpl implements DeviceChange_Registered {
  const _$DeviceChange_RegisteredImpl({required this.id, required this.name});

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
            other is _$DeviceChange_RegisteredImpl &&
            (identical(other.id, id) || other.id == id) &&
            (identical(other.name, name) || other.name == name));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id, name);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_RegisteredImplCopyWith<_$DeviceChange_RegisteredImpl>
      get copyWith => __$$DeviceChange_RegisteredImplCopyWithImpl<
          _$DeviceChange_RegisteredImpl>(this, _$identity);

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
      required final String name}) = _$DeviceChange_RegisteredImpl;

  @override
  DeviceId get id;
  String get name;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_RegisteredImplCopyWith<_$DeviceChange_RegisteredImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$DeviceChange_DisconnectedImplCopyWith<$Res>
    implements $DeviceChangeCopyWith<$Res> {
  factory _$$DeviceChange_DisconnectedImplCopyWith(
          _$DeviceChange_DisconnectedImpl value,
          $Res Function(_$DeviceChange_DisconnectedImpl) then) =
      __$$DeviceChange_DisconnectedImplCopyWithImpl<$Res>;
  @override
  @useResult
  $Res call({DeviceId id});
}

/// @nodoc
class __$$DeviceChange_DisconnectedImplCopyWithImpl<$Res>
    extends _$DeviceChangeCopyWithImpl<$Res, _$DeviceChange_DisconnectedImpl>
    implements _$$DeviceChange_DisconnectedImplCopyWith<$Res> {
  __$$DeviceChange_DisconnectedImplCopyWithImpl(
      _$DeviceChange_DisconnectedImpl _value,
      $Res Function(_$DeviceChange_DisconnectedImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? id = null,
  }) {
    return _then(_$DeviceChange_DisconnectedImpl(
      id: null == id
          ? _value.id
          : id // ignore: cast_nullable_to_non_nullable
              as DeviceId,
    ));
  }
}

/// @nodoc

class _$DeviceChange_DisconnectedImpl implements DeviceChange_Disconnected {
  const _$DeviceChange_DisconnectedImpl({required this.id});

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
            other is _$DeviceChange_DisconnectedImpl &&
            (identical(other.id, id) || other.id == id));
  }

  @override
  int get hashCode => Object.hash(runtimeType, id);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$DeviceChange_DisconnectedImplCopyWith<_$DeviceChange_DisconnectedImpl>
      get copyWith => __$$DeviceChange_DisconnectedImplCopyWithImpl<
          _$DeviceChange_DisconnectedImpl>(this, _$identity);

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
      _$DeviceChange_DisconnectedImpl;

  @override
  DeviceId get id;
  @override
  @JsonKey(ignore: true)
  _$$DeviceChange_DisconnectedImplCopyWith<_$DeviceChange_DisconnectedImpl>
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
