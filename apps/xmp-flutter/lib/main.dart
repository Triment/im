import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
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
      const MaterialApp(
        home: Scaffold(
          body: Center(
            child: Padding(
              padding: EdgeInsets.all(24),
              child: Text(
                'Rust 核心加载失败，请检查动态库构建配置。',
                textAlign: TextAlign.center,
              ),
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
    return MaterialApp(
      title: 'XMP',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.blue,
          brightness: Brightness.light,
        ),
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.blue,
          brightness: Brightness.dark,
        ),
        useMaterial3: true,
      ),
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
        Toasts(toasts: store.toasts),
      ],
    );
  }
}
