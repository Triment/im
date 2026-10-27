import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;
import 'api.dart';
import 'store.dart';
import 'screens/home_screen.dart';
import 'screens/login_screen.dart';
import 'widgets/toasts.dart';
import 'src/rust/frb_generated.dart/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  try {
    await RustLib.init();

    runApp(
      MultiProvider(
        providers: [
          ChangeNotifierProvider(
            create: (_) => XmpStore(RustApi()),
          ),
        ],
        child: const XmpApp(),
      ),
    );
  } catch (e, stack) {
    debugPrint('Rust 初始化失败: $e');
    debugPrintStack(stackTrace: stack);

    runApp(
      const shad.ShadcnApp(
        home: Center(
            child: Padding(
              padding: EdgeInsets.all(24),
              child: Text(
                'Rust 核心加载失败，请检查动态库构建配置。',
                textAlign: TextAlign.center,
              ),
            ),
          ),
      ),
    );
  }
}

class XmpApp extends StatelessWidget {
  const XmpApp({super.key});

  @override
  Widget build(BuildContext context) {
    return shad.ShadcnApp(
      title: 'XMP',
      theme: const shad.ThemeData(colorScheme: shad.ColorSchemes.lightZinc),
      darkTheme: const shad.ThemeData(colorScheme: shad.ColorSchemes.darkZinc),
      themeMode: shad.ThemeMode.system,
      home: const MainNavigator(),
    );
  }
}

class MainNavigator extends StatelessWidget {
  const MainNavigator({super.key});

  @override
  Widget build(BuildContext context) {
    final store = context.watch<XmpStore>();

    return Stack(
      children: [
        store.me == null ? const LoginScreen() : const HomeScreen(),
        if (store.incoming != null)
          Positioned(
            top: 24,
            left: 24,
            right: 24,
            child: shad.Card(
              child: Row(
                children: [
                  Icon(store.incoming!.video ? shad.LucideIcons.video : shad.LucideIcons.phone),
                  const SizedBox(width: 12),
                  const Expanded(child: Text('收到来电')),
                  shad.IconButton(
                    variance: shad.ButtonVariance.ghost,
                    icon: const Icon(shad.LucideIcons.phone),
                    onPressed: store.acceptIncoming,
                  ),
                  shad.IconButton(
                    variance: shad.ButtonVariance.ghost,
                    icon: const Icon(shad.LucideIcons.phoneOff),
                    onPressed: store.rejectIncoming,
                  ),
                ],
              ),
            ),
          ),
        Toasts(toasts: store.toasts),
      ],
    );
  }
}
