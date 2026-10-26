import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:intl/intl.dart';
import '../store.dart';
import '../models.dart';

class ChatView extends StatefulWidget {
  const ChatView({super.key});

  @override
  State<ChatView> createState() => _ChatViewState();
}

class _ChatViewState extends State<ChatView> {
  final _controller = TextEditingController();
  final _scrollController = ScrollController();

  void _send() {
    final text = _controller.text;
    if (text.trim().isEmpty) return;
    context.read<XmpStore>().sendText(text);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final store = context.watch<XmpStore>();
    final peer = store.selected;
    if (peer == null) {
      return const Center(child: Text('选择一个联系人开始聊天'));
    }

    final friend = store.friends.firstWhere((f) => f.id == peer);
    final chat = store.chats[peer];
    final msgs = chat?.msgs ?? [];

    return Column(
      children: [
        _buildHeader(context, friend, store.online[peer] ?? false),
        Expanded(
          child: ListView.builder(
            controller: _scrollController,
            padding: const EdgeInsets.all(16),
            itemCount: msgs.length,
            itemBuilder: (context, index) {
              final msg = msgs[index];
              final isMe = msg.sender == store.me?['user_id'];
              return _buildMessage(context, msg, isMe);
            },
          ),
        ),
        _buildInput(context),
      ],
    );
  }

  Widget _buildHeader(BuildContext context, Friend friend, bool isOnline) {
    return AppBar(
      title: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(friend.displayName),
          Text(
            isOnline ? '在线' : '离线',
            style: const TextStyle(fontSize: 12),
          ),
        ],
      ),
      actions: [
        IconButton(icon: const Icon(Icons.call), onPressed: () {}),
        IconButton(icon: const Icon(Icons.videocam), onPressed: () {}),
      ],
    );
  }

  Widget _buildMessage(BuildContext context, Msg msg, bool isMe) {
    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: isMe ? Theme.of(context).primaryColor : Theme.of(context).cardColor,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              msg.text ?? '',
              style: TextStyle(
                color: isMe ? Colors.white : null,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              DateFormat('HH:mm').format(DateTime.fromMillisecondsSinceEpoch(msg.ts)),
              style: TextStyle(
                fontSize: 10,
                color: isMe ? Colors.white70 : Colors.grey,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildInput(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Row(
        children: [
          IconButton(icon: const Icon(Icons.attach_file), onPressed: () {}),
          Expanded(
            child: TextField(
              controller: _controller,
              decoration: const InputDecoration(
                hintText: '输入消息...',
                border: OutlineInputBorder(),
              ),
              onSubmitted: (_) => _send(),
            ),
          ),
          IconButton(icon: const Icon(Icons.send), onPressed: _send),
        ],
      ),
    );
  }
}
