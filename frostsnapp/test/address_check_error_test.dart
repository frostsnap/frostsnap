import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/address.dart';

const _error = 'Invalid address for this network';

/// Pumped without a [WalletContext], which makes every non-empty entry invalid —
/// the same branch a malformed or wrong-network address takes, and reachable
/// without the Rust bridge.
Widget _page() => const MaterialApp(home: Scaffold(body: CheckAddressPage()));

void main() {
  testWidgets('the error appears on the first invalid entry', (tester) async {
    await tester.pumpWidget(_page());

    await tester.enterText(find.byType(TextFormField), 'notanaddress');
    await tester.pump();

    expect(find.text(_error), findsOneWidget);
  });

  testWidgets('the error clears when the field is emptied', (tester) async {
    await tester.pumpWidget(_page());

    await tester.enterText(find.byType(TextFormField), 'notanaddress');
    await tester.pump();
    // Asserted before clearing so the test cannot pass by the error never having
    // been shown — which is how it passed against the unfixed page.
    expect(find.text(_error), findsOneWidget);

    await tester.enterText(find.byType(TextFormField), '');
    await tester.pump();

    expect(find.text(_error), findsNothing);
  });
}
