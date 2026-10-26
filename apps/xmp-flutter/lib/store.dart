import 'dart:async';
import 'package:flutter/foundation.dart';
import 'models.dart';
import 'api.dart';

class XmpStore extends ChangeNotifier {
  final XmpApi api;
  
  Map<String, dynamic>? me;
  List<Friend> friends = [];
  Map<int, bool> online = {};
  Map<int, Chat> chats = {};
  int? selected;
  Incoming? incoming;
  ActiveCall? call;
  List<Toast> toasts = [];
  bool busy = false;

  XmpStore(this.api) {
    api.onEvent.listen(handleEvent);
  }

  void toast(String text, {String kind = "info"}) {
    final id = DateTime.now().millisecondsSinceEpoch + Random().nextDouble();
    toasts.add(Toast(id: id, text: text, kind: kind));
    notifyListeners();
    Future.delayed(const Duration(milliseconds: 4200), () {
      toasts.removeWhere((t) => t.id == id);
      notifyListeners();
    });
  }

  void patchChat(int peer, Chat Function(Chat) f) {
    chats[peer] = f(chats[peer] ?? Chat(msgs: [], unread: 0));
    notifyListeners();
  }

  void handleEvent(Map<String, dynamic> e) {
    switch (e['type']) {
      case "message":
        final m = Msg.fromJson(e['message']);
        if (m.sender == me?['user_id']) return;
        patchChat(m.sender, (c) {
          if (c.msgs.any((x) => x.seq == m.seq && x.conv == m.conv)) return c;
          c.convId = m.conv;
          c.msgs.add(m);
          c.unread = (selected == m.sender) ? 0 : c.unread + 1;
          return c;
        });
        break;
      case "presence":
        online[e['user']] = e['online'];
        notifyListeners();
        break;
      case "incoming_call":
        if (call != null) break;
        incoming = Incoming(callId: e['call_id'], peer: e['peer'], video: e['video']);
        notifyListeners();
        break;
      case "call_ended":
        if (incoming?.callId == e['call_id']) {
          incoming = null;
          notifyListeners();
        }
        if (call != null && call!.peer == e['peer']) {
          toast("通话已结束");
          endCallLocal(true);
        }
        break;
    }
  }

  Future<void> endCallLocal(bool notifyBackend) async {
    if (call == null) return;
    call = null;
    notifyListeners();
    if (notifyBackend) {
      try { await api.callHangup(); } catch (_) {}
    }
  }

  Future<bool> login(Map<String, dynamic> args) async {
    busy = true;
    notifyListeners();
    try {
      final r = await api.connect(args);
      me = r;
      final fs = await api.friends();
      friends = fs;
      final convs = await api.conversations();
      for (var c in convs) {
        final msgs = await api.sync(c.conv, 0);
        chats[c.peer] = Chat(convId: c.conv, msgs: msgs, unread: 0);
      }
      final first = fs.firstWhere((f) => f.status == FriendStatus.friends, orElse: () => fs.first);
      selected = first.id;
      busy = false;
      notifyListeners();
      return true;
    } catch (e) {
      toast(e.toString(), kind: "error");
      busy = false;
      notifyListeners();
      return false;
    }
  }

  Future<void> logout() async {
    await endCallLocal(true);
    try { await api.logout(); } catch (_) {}
    me = null;
    friends = [];
    chats = {};
    selected = null;
    online = {};
    notifyListeners();
  }

  void selectPeer(int id) {
    selected = id;
    patchChat(id, (c) {
      c.unread = 0;
      return c;
    });
    notifyListeners();
  }

  Future<void> sendText(String text) async {
    final peer = selected;
    final my = me;
    if (peer == null || my == null || text.trim().isEmpty) return;
    
    final tmp = Msg(
      conv: chats[peer]?.convId ?? 0,
      seq: -DateTime.now().millisecondsSinceEpoch,
      sender: my['user_id'],
      kind: 0,
      ts: DateTime.now().millisecondsSinceEpoch,
      text: text,
    );
    
    patchChat(peer, (c) {
      c.msgs.add(tmp);
      return c;
    });

    try {
      final ack = await api.sendText(peer, text);
      patchChat(peer, (c) {
        final index = c.msgs.indexWhere((m) => m.seq == tmp.seq);
        if (index != -1) {
          c.msgs[index] = Msg(
            conv: ack['conv'],
            seq: ack['seq'],
            sender: tmp.sender,
            kind: tmp.kind,
            ts: ack['ts'],
            text: tmp.text,
          );
        }
        c.convId = ack['conv'];
        return c;
      });
    } catch (e) {
      patchChat(peer, (c) {
        c.msgs.removeWhere((m) => m.seq == tmp.seq);
        return c;
      });
      toast("发送失败：$e", kind: "error");
    }
  }

  List<Friend> get friendsOnly => friends.where((f) => f.status == FriendStatus.friends).toList();
  List<Friend> get requests => friends.where((f) => f.status == FriendStatus.received).toList();
}
