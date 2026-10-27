// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:flutter_test/flutter_test.dart';
import 'package:provider/provider.dart';

import 'package:xmp_flutter/main.dart';
import 'package:xmp_flutter/api.dart';
import 'package:xmp_flutter/store.dart';

void main() {
  testWidgets('login screen renders connection fields', (WidgetTester tester) async {
    // Build our app and trigger a frame.
    await tester.pumpWidget(ChangeNotifierProvider(
      create: (_) => XmpStore(DemoApi()),
      child: const XmpApp(),
    ));

    expect(find.text('服务器地址'), findsOneWidget);
    expect(find.text('TLS Server Name'), findsOneWidget);
    expect(find.text('选择证书'), findsOneWidget);
    expect(find.text('使用已有 token 登录'), findsOneWidget);
  });
}
