import 'package:file_picker/file_picker.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/intl.dart';
import 'package:provider/provider.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;

import '../models.dart';
import '../store.dart';

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
    if (peer == null) return const Center(child: Text('选择一个联系人开始聊天'));
    final friend = store.friends.firstWhere((f) => f.id == peer);
    final msgs = store.chats[peer]?.msgs ?? <Msg>[];
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
              return _buildMessage(context, msg, msg.sender == store.me?['user_id']);
            },
          ),
        ),
        _buildInput(context),
      ],
    );
  }

  Widget _buildHeader(BuildContext context, Friend friend, bool isOnline) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(border: Border(bottom: BorderSide(color: shad.Theme.of(context).colorScheme.border))),
      child: Row(
        children: [
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(friend.displayName),
              Text(isOnline ? '在线' : '离线', style: TextStyle(fontSize: 12, color: shad.Theme.of(context).colorScheme.mutedForeground)),
            ]),
          ),
          shad.IconButton(variance: shad.ButtonVariance.ghost, icon: const Icon(shad.LucideIcons.phone), onPressed: () => context.read<XmpStore>().startCall(false)),
          shad.IconButton(variance: shad.ButtonVariance.ghost, icon: const Icon(shad.LucideIcons.video), onPressed: () => context.read<XmpStore>().startCall(true)),
        ],
      ),
    );
  }

  Widget _buildMessage(BuildContext context, Msg msg, bool isMe) {
    final background = isMe ? shad.Theme.of(context).colorScheme.primary : shad.Theme.of(context).colorScheme.muted;
    final foreground = isMe ? shad.Theme.of(context).colorScheme.primaryForeground : shad.Theme.of(context).colorScheme.foreground;
    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: shad.Card(
        theme: shad.CardTheme(fillColor: background, filled: true),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(msg.text ?? msg.file?.name ?? '文件', style: TextStyle(color: foreground)),
            const SizedBox(height: 4),
            Text(DateFormat('HH:mm').format(DateTime.fromMillisecondsSinceEpoch(msg.ts)), style: TextStyle(fontSize: 10, color: foreground.withValues(alpha: .7))),
          ]),
        ),
      ),
    );
  }

  Widget _buildInput(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8),
      child: Row(
        children: [
          shad.IconButton(
            variance: shad.ButtonVariance.ghost,
            icon: const Icon(shad.LucideIcons.paperclip),
            onPressed: () async {
              final result = await FilePicker.pickFiles();
              final path = result.isEmpty ? null : result.single.path;
              if (path != null && context.mounted) await context.read<XmpStore>().sendFile(path);
            },
          ),
          Expanded(child: shad.TextField(controller: _controller, placeholder: const Text('输入消息...'), onSubmitted: (_) => _send())),
          const SizedBox(width: 8),
          shad.IconButton(variance: shad.ButtonVariance.primary, icon: const Icon(shad.LucideIcons.send), onPressed: _send),
        ],
      ),
    );
  }
}
