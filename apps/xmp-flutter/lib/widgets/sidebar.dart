import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;

import '../store.dart';
import 'avatar.dart';

class Sidebar extends StatelessWidget {
  const Sidebar({super.key});

  @override
  Widget build(BuildContext context) {
    final store = context.watch<XmpStore>();
    final border = shad.Theme.of(context).colorScheme.border;
    return Container(
      width: 320,
      decoration: BoxDecoration(border: Border(right: BorderSide(color: border))),
      child: Column(
        children: [
          _buildHeader(context, store),
          const shad.Divider(),
          Expanded(
            child: ListView.builder(
              itemCount: store.friendsOnly.length,
              itemBuilder: (context, index) {
                final friend = store.friendsOnly[index];
                final chat = store.chats[friend.id];
                final selected = store.selected == friend.id;
                return GestureDetector(
                  onTap: () => store.selectPeer(friend.id),
                  child: Container(
                    color: selected ? shad.Theme.of(context).colorScheme.muted : null,
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                    child: Row(
                      children: [
                        Avatar(name: friend.displayName, online: store.online[friend.id] ?? false),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(friend.displayName),
                              Text(chat?.msgs.isNotEmpty == true ? chat!.msgs.last.text ?? '' : '', maxLines: 1, overflow: TextOverflow.ellipsis),
                            ],
                          ),
                        ),
                        if (chat != null && chat.unread > 0) shad.PrimaryBadge(child: Text(chat.unread.toString())),
                      ],
                    ),
                  ),
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
      padding: const EdgeInsets.all(16),
      child: Row(
        children: [
          Avatar(name: store.me?['username'] ?? 'Me'),
          const SizedBox(width: 12),
          Expanded(child: Text(store.me?['username'] ?? 'Me')),
          shad.IconButton(variance: shad.ButtonVariance.ghost, icon: const Icon(shad.LucideIcons.logOut), onPressed: store.logout),
        ],
      ),
    );
  }
}
