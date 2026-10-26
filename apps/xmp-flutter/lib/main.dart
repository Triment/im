import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'api.dart';
import 'store.dart';
import 'screens/home_screen.dart';
import 'screens/login_screen.dart';
import 'widgets/toasts.dart';

void main() {
  runApp(
    MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => XmpStore(DemoApi())),
      ],
      child: const XmpApp(),
    ),
  );
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
