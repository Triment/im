import 'package:flutter/material.dart';
import '../models.dart';

class Toasts extends StatelessWidget {
  final List<Toast> toasts;

  const Toasts({super.key, required this.toasts});

  @override
  Widget build(BuildContext context) {
    if (toasts.isEmpty) return const SizedBox.shrink();

    return Positioned(
      bottom: 24,
      left: 0,
      right: 0,
      child: Column(
        children: toasts.map((t) => _buildToast(context, t)).toList(),
      ),
    );
  }

  Widget _buildToast(BuildContext context, Toast toast) {
    final isError = toast.kind == 'error';
    return Container(
      margin: const EdgeInsets.symmetric(vertical: 4),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: BoxDecoration(
        color: isError ? Colors.red : Colors.black87,
        borderRadius: BorderRadius.circular(20),
      ),
      child: Text(
        toast.text,
        style: const TextStyle(color: Colors.white),
      ),
    );
  }
}
