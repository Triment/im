enum FriendStatus { sent, received, friends }

class Friend {
  final int id;
  final String username;
  final String displayName;
  FriendStatus status;

  Friend({
    required this.id,
    required this.username,
    required this.displayName,
    required this.status,
  });

  factory Friend.fromJson(Map<String, dynamic> json) {
    return Friend(
      id: json['id'],
      username: json['username'],
      displayName: json['display_name'],
      status: _parseStatus(json['status']),
    );
  }

  static FriendStatus _parseStatus(String status) {
    switch (status) {
      case 'sent':
        return FriendStatus.sent;
      case 'received':
        return FriendStatus.received;
      case 'friends':
        return FriendStatus.friends;
      default:
        return FriendStatus.friends;
    }
  }
}

class FileRef {
  final String name;
  final int size;
  final String hash;

  FileRef({required this.name, required this.size, required this.hash});

  factory FileRef.fromJson(Map<String, dynamic> json) {
    return FileRef(
      name: json['name'],
      size: json['size'],
      hash: json['hash'],
    );
  }
}

class Msg {
  final int conv;
  final int seq;
  final int sender;
  final int kind;
  final int ts;
  final String? text;
  final FileRef? file;

  Msg({
    required this.conv,
    required this.seq,
    required this.sender,
    required this.kind,
    required this.ts,
    this.text,
    this.file,
  });

  factory Msg.fromJson(Map<String, dynamic> json) {
    return Msg(
      conv: json['conv'],
      seq: json['seq'],
      sender: json['sender'],
      kind: json['kind'],
      ts: json['ts'],
      text: json['text'],
      file: json['file'] != null ? FileRef.fromJson(json['file']) : null,
    );
  }
}

class Conv {
  final int conv;
  final int peer;
  final int lastSeq;

  Conv({required this.conv, required this.peer, required this.lastSeq});

  factory Conv.fromJson(Map<String, dynamic> json) {
    return Conv(
      conv: json['conv'],
      peer: json['peer'],
      lastSeq: json['last_seq'],
    );
  }
}

class Chat {
  int? convId;
  List<Msg> msgs;
  int unread;

  Chat({this.convId, required this.msgs, this.unread = 0});
}

class Incoming {
  final String callId;
  final int peer;
  final bool video;

  Incoming({required this.callId, required this.peer, required this.video});
}

enum CallStatus { calling, connected }

class ActiveCall {
  final int peer;
  final bool video;
  CallStatus status;
  String? sas;
  int? startedAt;
  bool muted;
  bool camera;
  bool hd;
  bool minimized;

  ActiveCall({
    required this.peer,
    required this.video,
    this.status = CallStatus.calling,
    this.sas,
    this.startedAt,
    this.muted = false,
    this.camera = false,
    this.hd = false,
    this.minimized = false,
  });
}

class Toast {
  final double id;
  final String text;
  final String kind;

  Toast({required this.id, required this.text, required this.kind});
}
