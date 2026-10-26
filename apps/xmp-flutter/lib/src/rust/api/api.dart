import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'frb_generated.dart';

// This is a simplified version of what FRB would generate.
// In a real project, you'd run `flutter_rust_bridge_codegen`.

Future<MeDto> connect({
  required String server,
  required String serverName,
  String? certB64,
  required String mode,
  String? username,
  String? password,
  String? token,
  required StreamSink<XmpEvent> sink,
}) =>
    RustLib.instance.api.connect(
      server: server,
      serverName: serverName,
      certB64: certB64,
      mode: mode,
      username: username,
      password: password,
      token: token,
      sink: sink,
    );

Future<void> logout() => RustLib.instance.api.logout();

Future<List<FriendDto>> getFriends() => RustLib.instance.api.getFriends();

Future<ConvDto> sendText({required int to, required String text}) =>
    RustLib.instance.api.sendText(to: to, text: text);

// ... DTO definitions ...
class MeDto {
  final int userId;
  final String token;
  MeDto({required this.userId, required this.token});
}

class FriendDto {
  final int id;
  final String username;
  final String displayName;
  final String status;
  FriendDto({required this.id, required this.username, required this.displayName, required this.status});
}

class ConvDto {
  final int conv;
  final int peer;
  final int lastSeq;
  ConvDto({required this.conv, required this.peer, required this.lastSeq});
}

abstract class XmpEvent {
  const XmpEvent();
}

class XmpEvent_Message extends XmpEvent {
  final MsgDto message;
  const XmpEvent_Message({required this.message});
}

class MsgDto {
  final int conv;
  final int seq;
  final int sender;
  final int kind;
  final int ts;
  final String? text;
  MsgDto({required this.conv, required this.seq, required this.sender, required this.kind, required this.ts, this.text});
}
// ... and so on ...
