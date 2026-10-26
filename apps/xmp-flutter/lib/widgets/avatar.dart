import 'package:flutter/material.dart';

class Avatar extends StatelessWidget {
  final String name;
  final bool online;
  final double size;

  const Avatar({
    super.key,
    required this.name,
    this.online = false,
    this.size = 40,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        CircleAvatar(
          radius: size / 2,
          child: Text(name.isNotEmpty ? name[0].toUpperCase() : '?'),
        ),
        if (online)
          Positioned(
            right: 0,
            bottom: 0,
            child: Container(
              width: 12,
              height: 12,
              decoration: BoxDecoration(
                color: Colors.green,
                shape: BoxShape.circle,
                border: Border.all(color: Colors.white, width: 2),
              ),
            ),
          ),
      ],
    );
  }
}
