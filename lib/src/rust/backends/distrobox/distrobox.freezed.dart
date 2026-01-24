// dart format width=80
// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'distrobox.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$Status {
  String get field0;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $StatusCopyWith<Status> get copyWith =>
      _$StatusCopyWithImpl<Status>(this as Status, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is Status &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'Status(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $StatusCopyWith<$Res> {
  factory $StatusCopyWith(Status value, $Res Function(Status) _then) =
      _$StatusCopyWithImpl;
  @useResult
  $Res call({String field0});
}

/// @nodoc
class _$StatusCopyWithImpl<$Res> implements $StatusCopyWith<$Res> {
  _$StatusCopyWithImpl(this._self, this._then);

  final Status _self;
  final $Res Function(Status) _then;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_self.copyWith(
      field0: null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class Status_Up extends Status {
  const Status_Up(this.field0) : super._();

  @override
  final String field0;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $Status_UpCopyWith<Status_Up> get copyWith =>
      _$Status_UpCopyWithImpl<Status_Up>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is Status_Up &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'Status.up(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $Status_UpCopyWith<$Res> implements $StatusCopyWith<$Res> {
  factory $Status_UpCopyWith(Status_Up value, $Res Function(Status_Up) _then) =
      _$Status_UpCopyWithImpl;
  @override
  @useResult
  $Res call({String field0});
}

/// @nodoc
class _$Status_UpCopyWithImpl<$Res> implements $Status_UpCopyWith<$Res> {
  _$Status_UpCopyWithImpl(this._self, this._then);

  final Status_Up _self;
  final $Res Function(Status_Up) _then;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @pragma('vm:prefer-inline')
  $Res call({
    Object? field0 = null,
  }) {
    return _then(Status_Up(
      null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class Status_Created extends Status {
  const Status_Created(this.field0) : super._();

  @override
  final String field0;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $Status_CreatedCopyWith<Status_Created> get copyWith =>
      _$Status_CreatedCopyWithImpl<Status_Created>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is Status_Created &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'Status.created(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $Status_CreatedCopyWith<$Res>
    implements $StatusCopyWith<$Res> {
  factory $Status_CreatedCopyWith(
          Status_Created value, $Res Function(Status_Created) _then) =
      _$Status_CreatedCopyWithImpl;
  @override
  @useResult
  $Res call({String field0});
}

/// @nodoc
class _$Status_CreatedCopyWithImpl<$Res>
    implements $Status_CreatedCopyWith<$Res> {
  _$Status_CreatedCopyWithImpl(this._self, this._then);

  final Status_Created _self;
  final $Res Function(Status_Created) _then;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @pragma('vm:prefer-inline')
  $Res call({
    Object? field0 = null,
  }) {
    return _then(Status_Created(
      null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class Status_Exited extends Status {
  const Status_Exited(this.field0) : super._();

  @override
  final String field0;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $Status_ExitedCopyWith<Status_Exited> get copyWith =>
      _$Status_ExitedCopyWithImpl<Status_Exited>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is Status_Exited &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'Status.exited(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $Status_ExitedCopyWith<$Res>
    implements $StatusCopyWith<$Res> {
  factory $Status_ExitedCopyWith(
          Status_Exited value, $Res Function(Status_Exited) _then) =
      _$Status_ExitedCopyWithImpl;
  @override
  @useResult
  $Res call({String field0});
}

/// @nodoc
class _$Status_ExitedCopyWithImpl<$Res>
    implements $Status_ExitedCopyWith<$Res> {
  _$Status_ExitedCopyWithImpl(this._self, this._then);

  final Status_Exited _self;
  final $Res Function(Status_Exited) _then;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @pragma('vm:prefer-inline')
  $Res call({
    Object? field0 = null,
  }) {
    return _then(Status_Exited(
      null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class Status_Other extends Status {
  const Status_Other(this.field0) : super._();

  @override
  final String field0;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $Status_OtherCopyWith<Status_Other> get copyWith =>
      _$Status_OtherCopyWithImpl<Status_Other>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is Status_Other &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'Status.other(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $Status_OtherCopyWith<$Res>
    implements $StatusCopyWith<$Res> {
  factory $Status_OtherCopyWith(
          Status_Other value, $Res Function(Status_Other) _then) =
      _$Status_OtherCopyWithImpl;
  @override
  @useResult
  $Res call({String field0});
}

/// @nodoc
class _$Status_OtherCopyWithImpl<$Res> implements $Status_OtherCopyWith<$Res> {
  _$Status_OtherCopyWithImpl(this._self, this._then);

  final Status_Other _self;
  final $Res Function(Status_Other) _then;

  /// Create a copy of Status
  /// with the given fields replaced by the non-null parameter values.
  @override
  @pragma('vm:prefer-inline')
  $Res call({
    Object? field0 = null,
  }) {
    return _then(Status_Other(
      null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

// dart format on
