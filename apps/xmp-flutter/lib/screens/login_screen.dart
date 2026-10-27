import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:shadcn_flutter/shadcn_flutter.dart' as shad;

import '../store.dart';

class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key});

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  static const _serverKey = 'login.server';
  static const _serverNameKey = 'login.server_name';
  static const _certKey = 'login.cert_b64';
  static const _certNameKey = 'login.cert_name';
  static const _usernameKey = 'login.username';
  static const _passwordKey = 'login.password';
  static const _tokenKey = 'login.token';
  static const _useTokenKey = 'login.use_token';

  final _serverController = TextEditingController(text: 'rt.reqack.com:4433');
  final _serverNameController = TextEditingController(text: 'rt.reqack.com');
  final _certController = TextEditingController();
  final _tokenController = TextEditingController();
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  bool _isRegister = false;
  bool _useToken = false;
  String? _certFileName;

  @override
  void initState() {
    super.initState();
    _restoreForm();
  }

  Future<void> _restoreForm() async {
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    setState(() {
      _serverController.text = prefs.getString(_serverKey) ?? _serverController.text;
      _serverNameController.text = prefs.getString(_serverNameKey) ?? _serverNameController.text;
      _certController.text = prefs.getString(_certKey) ?? '';
      final certName = prefs.getString(_certNameKey);
      _certFileName = certName == null || certName.isEmpty ? null : certName;
      _usernameController.text = prefs.getString(_usernameKey) ?? '';
      _passwordController.text = prefs.getString(_passwordKey) ?? '';
      _tokenController.text = prefs.getString(_tokenKey) ?? '';
      _useToken = prefs.getBool(_useTokenKey) ?? false;
    });
  }

  Future<void> _persistForm() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_serverKey, _serverController.text.trim());
    await prefs.setString(_serverNameKey, _serverNameController.text.trim());
    await prefs.setString(_certKey, _certController.text.trim());
    if (_certFileName == null) {
      await prefs.remove(_certNameKey);
    } else {
      await prefs.setString(_certNameKey, _certFileName!);
    }
    await prefs.setString(_usernameKey, _usernameController.text.trim());
    await prefs.setString(_passwordKey, _passwordController.text);
    await prefs.setString(_tokenKey, _tokenController.text.trim());
    await prefs.setBool(_useTokenKey, _useToken);
  }

  Future<void> _pickCertificate() async {
    final result = await FilePicker.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['der', 'crt', 'cer', 'pem'],
    );
    if (result.isEmpty) return;
    final file = result.single;
    final bytes = await file.readAsBytes();
    if (bytes.isEmpty) {
      if (mounted) context.read<XmpStore>().toast('无法读取证书文件', kind: 'error');
      return;
    }
    final certificate = _decodeCertificate(bytes);
    setState(() {
      _certController.text = base64Encode(certificate);
      _certFileName = file.name;
    });
    await _persistForm();
  }

  List<int> _decodeCertificate(List<int> bytes) {
    final text = utf8.decode(bytes, allowMalformed: true);
    final begin = text.indexOf('-----BEGIN CERTIFICATE-----');
    final end = text.indexOf('-----END CERTIFICATE-----');
    if (begin == -1 || end == -1 || end <= begin) return bytes;
    final body = text
        .substring(begin + '-----BEGIN CERTIFICATE-----'.length, end)
        .replaceAll(RegExp(r'\s+'), '');
    try {
      return base64Decode(body);
    } on FormatException {
      return bytes;
    }
  }

  Future<void> _submit(XmpStore store) async {
    await _persistForm();
    await store.login({
      'server': _serverController.text.trim(),
      'server_name': _serverNameController.text.trim(),
      'cert_b64': _certController.text.trim().isEmpty ? null : _certController.text.trim(),
      'mode': _useToken ? 'resume' : (_isRegister ? 'register' : 'login'),
      'username': _usernameController.text.trim(),
      'password': _passwordController.text,
      'token': _tokenController.text.trim().isEmpty ? null : _tokenController.text.trim(),
    });
  }

  @override
  void dispose() {
    _serverController.dispose();
    _serverNameController.dispose();
    _certController.dispose();
    _tokenController.dispose();
    _usernameController.dispose();
    _passwordController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = context.read<XmpStore>();
    final isBusy = context.select((XmpStore s) => s.busy);
    final muted = shad.Theme.of(context).colorScheme.mutedForeground;

    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.symmetric(vertical: 24),
        child: Center(
          child: Container(
            constraints: const BoxConstraints(maxWidth: 440),
            padding: const EdgeInsets.all(24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const Text('XMP', style: TextStyle(fontSize: 32, fontWeight: FontWeight.bold)),
                const SizedBox(height: 8),
                const Text('连接到 XMP Server'),
                const SizedBox(height: 28),
                const Text('服务器地址'),
                const SizedBox(height: 6),
                shad.TextField(controller: _serverController, placeholder: const Text('host:port')),
                const SizedBox(height: 16),
                const Text('TLS Server Name'),
                const SizedBox(height: 6),
                shad.TextField(controller: _serverNameController, placeholder: const Text('证书域名')),
                const SizedBox(height: 16),
                Row(
                  children: [
                    Expanded(child: Text(_certFileName ?? '未选择服务器证书')),
                    shad.OutlineButton(
                      leading: const Icon(shad.LucideIcons.fileKey),
                      onPressed: _pickCertificate,
                      child: const Text('选择证书'),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Text('选择 DER/CRT/CER/PEM 文件后自动编码为 Base64', style: TextStyle(fontSize: 12, color: muted)),
                const SizedBox(height: 16),
                shad.Switch(
                  leading: const Text('使用已有 token 登录'),
                  value: _useToken,
                  onChanged: (value) => setState(() => _useToken = value),
                ),
                if (_useToken) ...[
                  const SizedBox(height: 8),
                  shad.TextField(controller: _tokenController, placeholder: const Text('Session token'), obscureText: true),
                ],
                if (!_useToken) ...[
                  const SizedBox(height: 16),
                  const Text('用户名'),
                  const SizedBox(height: 6),
                  shad.TextField(controller: _usernameController, placeholder: const Text('用户名')),
                  const SizedBox(height: 16),
                  const Text('密码'),
                  const SizedBox(height: 6),
                  shad.TextField(controller: _passwordController, placeholder: const Text('密码'), obscureText: true),
                ],
                const SizedBox(height: 24),
                shad.PrimaryButton(
                  onPressed: isBusy ? null : () => _submit(store),
                  leading: isBusy ? const shad.CircularProgressIndicator(size: 16) : null,
                  child: Text(_isRegister ? '注册' : '登录'),
                ),
                shad.GhostButton(
                  onPressed: () => setState(() => _isRegister = !_isRegister),
                  child: Text(_isRegister ? '已有账号？去登录' : '没有账号？去注册'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
