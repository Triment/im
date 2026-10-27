import 'package:flutter/widgets.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;

import '../models.dart';

class Toasts extends StatelessWidget {
  final List<Toast> toasts;

  const Toasts({super.key, required this.toasts});

  @override
  Widget build(BuildContext context) {
    if (toasts.isEmpty) return const SizedBox.shrink();
    return Positioned(
      bottom: 24,
      left: 16,
      right: 16,
      child: Column(
        children: toasts.map((toast) => Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: shad.Card(child: Text(toast.text)),
        )).toList(),
      ),
    );
  }
}
