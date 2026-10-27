import 'package:flutter/widgets.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;

class Avatar extends StatelessWidget {
  final String name;
  final bool online;
  final double size;

  const Avatar({super.key, required this.name, this.online = false, this.size = 40});

  @override
  Widget build(BuildContext context) {
    return shad.Avatar(
      initials: shad.Avatar.getInitials(name),
      size: size,
      badge: online ? const shad.AvatarBadge() : null,
    );
  }
}
