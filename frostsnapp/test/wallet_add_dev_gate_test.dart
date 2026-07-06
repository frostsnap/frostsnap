import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/wallet_add.dart';

// The remote (nostr) surface is developer-only. Without a
// SettingsContext the gate must fail CLOSED: no Join card, and the
// restore copy doesn't advertise recovering "from other people".
// (The dev-ON rendering needs a live FFI-backed SettingsContext and
// is covered by the flows' own tests.)

void main() {
  testWidgets('homepage hides remote surface when developer mode is off', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: WalletAddColumn(onPressed: (_) {})),
      ),
    );
    expect(find.text('Create a multi-sig wallet'), findsOneWidget);
    expect(find.text('Restore a wallet'), findsOneWidget);
    expect(find.text('Join with invite link'), findsNothing);
    expect(
      find.text('Use an existing device key or load a physical backup'),
      findsOneWidget,
    );
  });
}
