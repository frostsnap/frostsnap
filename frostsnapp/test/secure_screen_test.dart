import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/verifiable_card/card_platform.dart';
import 'package:frostsnap/verifiable_card/secure_screen.dart';
import 'package:google_fonts/google_fonts.dart';

// The route that shows or takes card words blocks capture on Android and
// keeps the screen on, and warns on Linux, where capture can't be blocked
// (design §5.2).

/// A card route: the flow's SecureScreen, with a nested one inside it.
Widget _route() => const MaterialApp(
  home: Scaffold(
    body: SecureScreen(child: SecureScreen(child: Text('words'))),
  ),
);

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  final calls = <(String, Object?)>[];
  setUp(() {
    calls.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SecureScreen.channel, (call) async {
          calls.add((call.method, call.arguments));
          return null;
        });
  });
  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SecureScreen.channel, null);
  });

  testWidgets('on Android, blocks capture and keeps the screen on', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    await tester.pumpWidget(_route());
    await tester.pump();
    // Once, though there are two of them.
    expect(calls, [('setSecure', true), ('keepScreenOn', true)]);
    expect(find.text(linuxCaptureWarning), findsNothing);

    calls.clear();
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    expect(calls, [('setSecure', false), ('keepScreenOn', false)]);
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('on Android, warns if the phone refuses to block capture', (
    tester,
  ) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SecureScreen.channel, (call) async {
          calls.add((call.method, call.arguments));
          throw PlatformException(code: 'refused');
        });
    await tester.pumpWidget(_route());
    await tester.pump();
    expect(calls, [('setSecure', true)]);
    expect(find.text(androidCaptureWarning), findsOneWidget);
    expect(find.text('words'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets("on Linux, warns that capture can't be blocked", (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    await tester.pumpWidget(_route());
    await tester.pump();
    // Once, though there are two of them.
    expect(find.text(linuxCaptureWarning), findsOneWidget);
    expect(calls, isEmpty);
    await tester.pumpWidget(const SizedBox());
    debugDefaultTargetPlatformOverride = null;
  });

  test('cardFlowsSupported is true on Android and Linux only', () {
    for (final platform in TargetPlatform.values) {
      debugDefaultTargetPlatformOverride = platform;
      expect(
        cardFlowsSupported,
        platform == TargetPlatform.android || platform == TargetPlatform.linux,
        reason: '$platform',
      );
    }
    debugDefaultTargetPlatformOverride = null;
  });
}
