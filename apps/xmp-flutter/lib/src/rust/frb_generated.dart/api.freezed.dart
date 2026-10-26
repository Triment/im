// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$XmpEvent {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is XmpEvent);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'XmpEvent()';
  }
}

/// @nodoc
class $XmpEventCopyWith<$Res> {
  $XmpEventCopyWith(XmpEvent _, $Res Function(XmpEvent) __);
}

/// Adds pattern-matching-related methods to [XmpEvent].
extension XmpEventPatterns on XmpEvent {
  /// A variant of `map` that fallback to returning `orElse`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmpEvent_Message value)? message,
    TResult Function(XmpEvent_Presence value)? presence,
    TResult Function(XmpEvent_IncomingCall value)? incomingCall,
    TResult Function(XmpEvent_CallEnded value)? callEnded,
    TResult Function(XmpEvent_Disconnected value)? disconnected,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message() when message != null:
        return message(_that);
      case XmpEvent_Presence() when presence != null:
        return presence(_that);
      case XmpEvent_IncomingCall() when incomingCall != null:
        return incomingCall(_that);
      case XmpEvent_CallEnded() when callEnded != null:
        return callEnded(_that);
      case XmpEvent_Disconnected() when disconnected != null:
        return disconnected(_that);
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// Callbacks receives the raw object, upcasted.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case final Subclass2 value:
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmpEvent_Message value) message,
    required TResult Function(XmpEvent_Presence value) presence,
    required TResult Function(XmpEvent_IncomingCall value) incomingCall,
    required TResult Function(XmpEvent_CallEnded value) callEnded,
    required TResult Function(XmpEvent_Disconnected value) disconnected,
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message():
        return message(_that);
      case XmpEvent_Presence():
        return presence(_that);
      case XmpEvent_IncomingCall():
        return incomingCall(_that);
      case XmpEvent_CallEnded():
        return callEnded(_that);
      case XmpEvent_Disconnected():
        return disconnected(_that);
    }
  }

  /// A variant of `map` that fallback to returning `null`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmpEvent_Message value)? message,
    TResult? Function(XmpEvent_Presence value)? presence,
    TResult? Function(XmpEvent_IncomingCall value)? incomingCall,
    TResult? Function(XmpEvent_CallEnded value)? callEnded,
    TResult? Function(XmpEvent_Disconnected value)? disconnected,
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message() when message != null:
        return message(_that);
      case XmpEvent_Presence() when presence != null:
        return presence(_that);
      case XmpEvent_IncomingCall() when incomingCall != null:
        return incomingCall(_that);
      case XmpEvent_CallEnded() when callEnded != null:
        return callEnded(_that);
      case XmpEvent_Disconnected() when disconnected != null:
        return disconnected(_that);
      case _:
        return null;
    }
  }

  /// A variant of `when` that fallback to an `orElse` callback.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(MsgDto message)? message,
    TResult Function(PlatformInt64 user, bool online)? presence,
    TResult Function(String callId, PlatformInt64 peer, bool video)?
        incomingCall,
    TResult Function(String callId, PlatformInt64 peer)? callEnded,
    TResult Function()? disconnected,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message() when message != null:
        return message(_that.message);
      case XmpEvent_Presence() when presence != null:
        return presence(_that.user, _that.online);
      case XmpEvent_IncomingCall() when incomingCall != null:
        return incomingCall(_that.callId, _that.peer, _that.video);
      case XmpEvent_CallEnded() when callEnded != null:
        return callEnded(_that.callId, _that.peer);
      case XmpEvent_Disconnected() when disconnected != null:
        return disconnected();
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// As opposed to `map`, this offers destructuring.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case Subclass2(:final field2):
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(MsgDto message) message,
    required TResult Function(PlatformInt64 user, bool online) presence,
    required TResult Function(String callId, PlatformInt64 peer, bool video)
        incomingCall,
    required TResult Function(String callId, PlatformInt64 peer) callEnded,
    required TResult Function() disconnected,
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message():
        return message(_that.message);
      case XmpEvent_Presence():
        return presence(_that.user, _that.online);
      case XmpEvent_IncomingCall():
        return incomingCall(_that.callId, _that.peer, _that.video);
      case XmpEvent_CallEnded():
        return callEnded(_that.callId, _that.peer);
      case XmpEvent_Disconnected():
        return disconnected();
    }
  }

  /// A variant of `when` that fallback to returning `null`
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(MsgDto message)? message,
    TResult? Function(PlatformInt64 user, bool online)? presence,
    TResult? Function(String callId, PlatformInt64 peer, bool video)?
        incomingCall,
    TResult? Function(String callId, PlatformInt64 peer)? callEnded,
    TResult? Function()? disconnected,
  }) {
    final _that = this;
    switch (_that) {
      case XmpEvent_Message() when message != null:
        return message(_that.message);
      case XmpEvent_Presence() when presence != null:
        return presence(_that.user, _that.online);
      case XmpEvent_IncomingCall() when incomingCall != null:
        return incomingCall(_that.callId, _that.peer, _that.video);
      case XmpEvent_CallEnded() when callEnded != null:
        return callEnded(_that.callId, _that.peer);
      case XmpEvent_Disconnected() when disconnected != null:
        return disconnected();
      case _:
        return null;
    }
  }
}

/// @nodoc

class XmpEvent_Message extends XmpEvent {
  const XmpEvent_Message({required this.message}) : super._();

  final MsgDto message;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $XmpEvent_MessageCopyWith<XmpEvent_Message> get copyWith =>
      _$XmpEvent_MessageCopyWithImpl<XmpEvent_Message>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is XmpEvent_Message &&
            (identical(other.message, message) || other.message == message));
  }

  @override
  int get hashCode {
    return Object.hash(runtimeType, message);
  }

  @override
  String toString() {
    return 'XmpEvent.message(message: $message)';
  }
}

/// @nodoc
abstract mixin class $XmpEvent_MessageCopyWith<$Res>
    implements $XmpEventCopyWith<$Res> {
  factory $XmpEvent_MessageCopyWith(
          XmpEvent_Message value, $Res Function(XmpEvent_Message) _then) =
      _$XmpEvent_MessageCopyWithImpl;
  @useResult
  $Res call({MsgDto message});
}

/// @nodoc
class _$XmpEvent_MessageCopyWithImpl<$Res>
    implements $XmpEvent_MessageCopyWith<$Res> {
  _$XmpEvent_MessageCopyWithImpl(this._self, this._then);

  final XmpEvent_Message _self;
  final $Res Function(XmpEvent_Message) _then;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? message = null,
  }) {
    return _then(XmpEvent_Message(
      message: null == message
          ? _self.message
          : message // ignore: cast_nullable_to_non_nullable
              as MsgDto,
    ));
  }
}

/// @nodoc

class XmpEvent_Presence extends XmpEvent {
  const XmpEvent_Presence({required this.user, required this.online})
      : super._();

  final PlatformInt64 user;
  final bool online;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $XmpEvent_PresenceCopyWith<XmpEvent_Presence> get copyWith =>
      _$XmpEvent_PresenceCopyWithImpl<XmpEvent_Presence>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is XmpEvent_Presence &&
            (identical(other.user, user) || other.user == user) &&
            (identical(other.online, online) || other.online == online));
  }

  @override
  int get hashCode {
    return Object.hash(runtimeType, user, online);
  }

  @override
  String toString() {
    return 'XmpEvent.presence(user: $user, online: $online)';
  }
}

/// @nodoc
abstract mixin class $XmpEvent_PresenceCopyWith<$Res>
    implements $XmpEventCopyWith<$Res> {
  factory $XmpEvent_PresenceCopyWith(
          XmpEvent_Presence value, $Res Function(XmpEvent_Presence) _then) =
      _$XmpEvent_PresenceCopyWithImpl;
  @useResult
  $Res call({PlatformInt64 user, bool online});
}

/// @nodoc
class _$XmpEvent_PresenceCopyWithImpl<$Res>
    implements $XmpEvent_PresenceCopyWith<$Res> {
  _$XmpEvent_PresenceCopyWithImpl(this._self, this._then);

  final XmpEvent_Presence _self;
  final $Res Function(XmpEvent_Presence) _then;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? user = null,
    Object? online = null,
  }) {
    return _then(XmpEvent_Presence(
      user: null == user
          ? _self.user
          : user // ignore: cast_nullable_to_non_nullable
              as PlatformInt64,
      online: null == online
          ? _self.online
          : online // ignore: cast_nullable_to_non_nullable
              as bool,
    ));
  }
}

/// @nodoc

class XmpEvent_IncomingCall extends XmpEvent {
  const XmpEvent_IncomingCall(
      {required this.callId, required this.peer, required this.video})
      : super._();

  final String callId;
  final PlatformInt64 peer;
  final bool video;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $XmpEvent_IncomingCallCopyWith<XmpEvent_IncomingCall> get copyWith =>
      _$XmpEvent_IncomingCallCopyWithImpl<XmpEvent_IncomingCall>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is XmpEvent_IncomingCall &&
            (identical(other.callId, callId) || other.callId == callId) &&
            (identical(other.peer, peer) || other.peer == peer) &&
            (identical(other.video, video) || other.video == video));
  }

  @override
  int get hashCode {
    return Object.hash(runtimeType, callId, peer, video);
  }

  @override
  String toString() {
    return 'XmpEvent.incomingCall(callId: $callId, peer: $peer, video: $video)';
  }
}

/// @nodoc
abstract mixin class $XmpEvent_IncomingCallCopyWith<$Res>
    implements $XmpEventCopyWith<$Res> {
  factory $XmpEvent_IncomingCallCopyWith(XmpEvent_IncomingCall value,
          $Res Function(XmpEvent_IncomingCall) _then) =
      _$XmpEvent_IncomingCallCopyWithImpl;
  @useResult
  $Res call({String callId, PlatformInt64 peer, bool video});
}

/// @nodoc
class _$XmpEvent_IncomingCallCopyWithImpl<$Res>
    implements $XmpEvent_IncomingCallCopyWith<$Res> {
  _$XmpEvent_IncomingCallCopyWithImpl(this._self, this._then);

  final XmpEvent_IncomingCall _self;
  final $Res Function(XmpEvent_IncomingCall) _then;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? callId = null,
    Object? peer = null,
    Object? video = null,
  }) {
    return _then(XmpEvent_IncomingCall(
      callId: null == callId
          ? _self.callId
          : callId // ignore: cast_nullable_to_non_nullable
              as String,
      peer: null == peer
          ? _self.peer
          : peer // ignore: cast_nullable_to_non_nullable
              as PlatformInt64,
      video: null == video
          ? _self.video
          : video // ignore: cast_nullable_to_non_nullable
              as bool,
    ));
  }
}

/// @nodoc

class XmpEvent_CallEnded extends XmpEvent {
  const XmpEvent_CallEnded({required this.callId, required this.peer})
      : super._();

  final String callId;
  final PlatformInt64 peer;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $XmpEvent_CallEndedCopyWith<XmpEvent_CallEnded> get copyWith =>
      _$XmpEvent_CallEndedCopyWithImpl<XmpEvent_CallEnded>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is XmpEvent_CallEnded &&
            (identical(other.callId, callId) || other.callId == callId) &&
            (identical(other.peer, peer) || other.peer == peer));
  }

  @override
  int get hashCode {
    return Object.hash(runtimeType, callId, peer);
  }

  @override
  String toString() {
    return 'XmpEvent.callEnded(callId: $callId, peer: $peer)';
  }
}

/// @nodoc
abstract mixin class $XmpEvent_CallEndedCopyWith<$Res>
    implements $XmpEventCopyWith<$Res> {
  factory $XmpEvent_CallEndedCopyWith(
          XmpEvent_CallEnded value, $Res Function(XmpEvent_CallEnded) _then) =
      _$XmpEvent_CallEndedCopyWithImpl;
  @useResult
  $Res call({String callId, PlatformInt64 peer});
}

/// @nodoc
class _$XmpEvent_CallEndedCopyWithImpl<$Res>
    implements $XmpEvent_CallEndedCopyWith<$Res> {
  _$XmpEvent_CallEndedCopyWithImpl(this._self, this._then);

  final XmpEvent_CallEnded _self;
  final $Res Function(XmpEvent_CallEnded) _then;

  /// Create a copy of XmpEvent
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? callId = null,
    Object? peer = null,
  }) {
    return _then(XmpEvent_CallEnded(
      callId: null == callId
          ? _self.callId
          : callId // ignore: cast_nullable_to_non_nullable
              as String,
      peer: null == peer
          ? _self.peer
          : peer // ignore: cast_nullable_to_non_nullable
              as PlatformInt64,
    ));
  }
}

/// @nodoc

class XmpEvent_Disconnected extends XmpEvent {
  const XmpEvent_Disconnected() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is XmpEvent_Disconnected);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'XmpEvent.disconnected()';
  }
}

// dart format on
