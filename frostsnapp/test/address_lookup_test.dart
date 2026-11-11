import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/address.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';

import 'support/rust_lib.dart';

const mainnetTaproot =
    'bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr';
const testnetTaproot =
    'tb1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c';

String withCharAt(String s, int index, String c) =>
    s.substring(0, index) + c + s.substring(index + 1);

class Step {
  final Address address;
  final int start;
  final result = Completer<AddressInfo?>();
  Step(this.address, this.start);
}

class Harness {
  final steps = <Step>[];
  final shown = <AddressInfo>[];
  String? scanned;

  Future<AddressInfo?> searchStep(Address address, int start) {
    final step = Step(address, start);
    steps.add(step);
    return step.result.future;
  }

  Widget build({BitcoinNetwork network = BitcoinNetwork.bitcoin}) =>
      MaterialApp(
        home: Scaffold(
          body: AddressLookup(
            network: network,
            searchStep: searchStep,
            onShowAddress: shown.add,
            onScan: (_) async => scanned,
          ),
        ),
      );
}

Future<void> enter(WidgetTester tester, String text) async {
  await tester.enterText(find.byKey(const Key('address-input')), text);
  await tester.pump();
}

Set<int> highlighted(WidgetTester tester) {
  final span = tester
      .state<EditableTextState>(find.byType(EditableText))
      .buildTextSpan();
  final offsets = <int>{};
  var offset = 0;
  span.visitChildren((child) {
    final text = (child as TextSpan).text ?? '';
    if (child.style?.backgroundColor != null) {
      for (var i = 0; i < text.length; i++) {
        offsets.add(offset + i);
      }
    }
    offset += text.length;
    return true;
  });
  return offsets;
}

void main() {
  setUpAll(initRustLibForTests);

  group('highlighting', () {
    testWidgets('the field highlights exactly the reported characters', (
      tester,
    ) async {
      final harness = Harness();
      await tester.pumpWidget(harness.build());
      final uri = 'bitcoin:${withCharAt(mainnetTaproot, 20, 'q')}';
      await enter(tester, uri);
      expect(highlighted(tester), {28});
      expect(harness.steps, isEmpty);
      expect(
        find.text(
          "Checksum doesn't match. The highlighted character looks wrong.",
        ),
        findsOneWidget,
      );

      await enter(tester, 'bitcoin:$mainnetTaproot');
      expect(highlighted(tester), isEmpty);
    });

    testWidgets('a scanned URI fills the field and starts the search', (
      tester,
    ) async {
      final harness = Harness()
        ..scanned = 'BITCOIN:${mainnetTaproot.toUpperCase()}';
      await tester.pumpWidget(harness.build());
      await tester.tap(find.byTooltip('Scan QR code'));
      await tester.pump();
      expect(find.text(harness.scanned!), findsOneWidget);
      expect(harness.steps.single.address.toString(), mainnetTaproot);
    });
  });

  group('search', () {
    Future<Harness> searchFor(WidgetTester tester, String text) async {
      final harness = Harness();
      await tester.pumpWidget(harness.build());
      await enter(tester, text);
      return harness;
    }

    Future<void> finish(
      WidgetTester tester,
      Step step,
      AddressInfo? result,
    ) async {
      step.result.complete(result);
      await tester.pumpAndSettle();
    }

    testWidgets('a valid address starts searching the first step by itself', (
      tester,
    ) async {
      final harness = Harness();
      await tester.pumpWidget(harness.build());
      await enter(tester, withCharAt(mainnetTaproot, 20, 'q'));
      await enter(tester, testnetTaproot);
      expect(harness.steps, isEmpty);

      await enter(tester, mainnetTaproot);
      expect(harness.steps.single.address.toString(), mainnetTaproot);
      expect(harness.steps.single.start, 0);
      expect(find.byKey(const Key('searching')), findsOneWidget);
      expect(
        find.text('Searching receive and change addresses #0 to #4,999'),
        findsOneWidget,
      );
    });

    testWidgets('not found offers to keep looking at the next step', (
      tester,
    ) async {
      final harness = await searchFor(tester, mainnetTaproot);
      await finish(tester, harness.steps.single, null);
      expect(find.byKey(const Key('search-not-found')), findsOneWidget);
      expect(
        find.text('Not among receive and change addresses #0 to #4,999.'),
        findsOneWidget,
      );

      await tester.tap(find.byKey(const Key('keep-looking')));
      await tester.pump();
      expect(harness.steps.last.start, 5000);
      expect(
        find.text('Searching receive and change addresses #5,000 to #9,999'),
        findsOneWidget,
      );

      await finish(tester, harness.steps.last, null);
      expect(
        find.text('Not among receive and change addresses #0 to #9,999.'),
        findsOneWidget,
      );
    });

    testWidgets('a match leads into the receive flow for that address', (
      tester,
    ) async {
      final harness = await searchFor(tester, mainnetTaproot);
      final address = SuperWallet.testAddressInfo();
      await finish(tester, harness.steps.single, address);
      expect(find.byKey(const Key('search-found')), findsOneWidget);
      expect(find.text('This is your receive address #24'), findsOneWidget);

      await tester.tap(find.byKey(const Key('show-found-address')));
      expect(harness.shown, [address]);
    });

    testWidgets('editing the address drops the result of the old search', (
      tester,
    ) async {
      final harness = await searchFor(tester, mainnetTaproot);
      final first = harness.steps.single;
      await enter(tester, withCharAt(mainnetTaproot, 20, 'q'));
      expect(find.byKey(const Key('searching')), findsNothing);

      await finish(tester, first, SuperWallet.testAddressInfo());
      expect(find.byKey(const Key('search-found')), findsNothing);
    });

    testWidgets('a failed step says so', (tester) async {
      final harness = await searchFor(tester, mainnetTaproot);
      harness.steps.single.result.completeError(StateError('boom'));
      await tester.pumpAndSettle();
      expect(find.text("Couldn't search"), findsOneWidget);
    });
  });
}
