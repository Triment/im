import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../store.dart';
import 'avatar.dart';

class Sidebar extends StatelessWidget {
  const Sidebar({super.key});

  @override
  Widget build(BuildContext context) {
    final store = context.watch<XmpStore>();

    return Container(
      decoration: BoxDecoration(
        border:
            Border(right: BorderSide(color: Theme.of(context).dividerColor)),
      ),
      child: Column(
        children: [
          _buildHeader(context, store),
          Expanded(
            child: ListView.builder(
              itemCount: store.friendsOnly.length,
              itemBuilder: (context, index) {
                final friend = store.friendsOnly[index];
                final chat = store.chats[friend.id];
                final isSelected = store.selected == friend.id;
                final isOnline = store.online[friend.id] ?? false;

                return ListTile(
                  leading: Avatar(
                    name: friend.displayName,
                    online: isOnline,
                  ),
                  title: Text(friend.displayName),
                  subtitle: Text(
                    chat?.msgs.last.text ?? '',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  trailing: chat != null && chat.unread > 0
                      ? Badge(label: Text(chat.unread.toString()))
                      : null,
                  selected: isSelected,
                  onTap: () => store.selectPeer(friend.id),
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildHeader(BuildContext context, XmpStore store) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Row(
        children: [
          Avatar(name: store.me?['username'] ?? 'Me'),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              store.me?['username'] ?? 'Me',
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
          ),
          IconButton(
            icon: const Icon(Icons.logout),
            onPressed: () => store.logout(),
          ),
        ],
      ),
    );
  }
}
