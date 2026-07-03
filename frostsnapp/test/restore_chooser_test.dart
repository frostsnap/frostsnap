import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/restore_chooser.dart';

// The Restore verb's mechanism fork is pure navigation — no FFI, no
// contexts. Callbacks are injected so the test never needs a live
// coord/nostr; the default (un-injected) behavior pops a
// RestoreChoice, covered by the pop test.

Widget _wrap(Widget child) => MaterialApp(home: Scaffold(body: child));

void main() {
  testWidgets('local card fires onLocal', (tester) async {
    var local = 0;
    var remote = 0;
    await tester.pumpWidget(
      _wrap(RestoreChooser(onLocal: () => local++, onRemote: () => remote++)),
    );
    await tester.tap(find.text('With your devices here'));
    await tester.pump();
    expect(local, 1);
    expect(remote, 0);
  });

  testWidgets('remote card fires onRemote', (tester) async {
    var local = 0;
    var remote = 0;
    await tester.pumpWidget(
      _wrap(RestoreChooser(onLocal: () => local++, onRemote: () => remote++)),
    );
    await tester.tap(find.text('With others'));
    await tester.pump();
    expect(local, 0);
    expect(remote, 1);
  });

  testWidgets('default behavior pops the chosen branch', (tester) async {
    RestoreChoice? popped;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async {
                  popped = await Navigator.of(context).push<RestoreChoice>(
                    MaterialPageRoute(builder: (_) => const RestoreChooser()),
                  );
                },
                child: const Text('open'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('With others'));
    await tester.pumpAndSettle();
    expect(popped, RestoreChoice.remote);
  });
}
