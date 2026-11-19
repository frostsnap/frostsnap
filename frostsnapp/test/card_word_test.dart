import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/verifiable_card/card_word.dart';
import 'package:google_fonts/google_fonts.dart';

// The final check bolds a word's first four letters: no two BIP39 words share
// them, so they are the letters a substituted lookalike can't match (spec
// L1.7.4).

/// Every leaf span of [root], with its text and its effective weight.
List<(String, FontWeight?)> leaves(InlineSpan root) {
  final out = <(String, FontWeight?)>[];
  void walk(InlineSpan span, TextStyle inherited) {
    if (span is! TextSpan) return;
    final style = inherited.merge(span.style);
    final text = span.text;
    if (text != null && text.isNotEmpty) out.add((text, style.fontWeight));
    for (final child in span.children ?? const <InlineSpan>[]) {
      walk(child, style);
    }
  }

  walk(root, const TextStyle());
  return out;
}

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  test('split puts the first four letters, in capitals, in the head', () {
    expect(CardWord.split('sizzle'), ('SIZZ', 'LE'));
    expect(CardWord.split('size'), ('SIZE', ''));
    expect(CardWord.split('act'), ('ACT', ''));
  });

  test('spellCardWord spells in capitals with non-breaking hyphens', () {
    // U+2011, so the spelled word never wraps at a hyphen.
    expect(spellCardWord('size'), 'S\u2011I\u2011Z\u2011E');
  });

  testWidgets('CardWord makes the first four letters bold', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: CardWord('sizzle'))),
    );
    final richText = tester.widget<RichText>(
      find.descendant(
        of: find.byType(CardWord),
        matching: find.byType(RichText),
      ),
    );
    final spans = leaves(richText.text);
    expect(spans.map((s) => s.$1).join(), 'SIZZLE');

    final head = spans.firstWhere((s) => s.$1 == 'SIZZ');
    final tail = spans.firstWhere((s) => s.$1 == 'LE');
    expect(head.$2, FontWeight.w800);
    expect(
      tail.$2 == null || tail.$2!.value < FontWeight.w600.value,
      isTrue,
      reason: 'the tail must not be bold',
    );
  });
}
