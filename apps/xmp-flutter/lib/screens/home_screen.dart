import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../store.dart';
import '../widgets/sidebar.dart';
import '../widgets/chat_view.dart';

class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final store = context.watch<XmpStore>();
    final isMobile = MediaQuery.of(context).size.width < 600;

    return Scaffold(
      body: Row(
        children: [
          if (!isMobile || store.selected == null)
            const SizedBox(
              width: 320,
              child: Sidebar(),
            ),
          if (!isMobile || store.selected != null)
            const Expanded(
              child: ChatView(),
            ),
        ],
      ),
    );
  }
}
