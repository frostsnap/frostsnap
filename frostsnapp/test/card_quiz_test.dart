import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/verifiable_card/card_quiz.dart';
import 'package:frostsnap/verifiable_card/card_word.dart';
import 'package:google_fonts/google_fonts.dart';

// The phone's quiz (design §5.2 P6, P11): the standard check-backup quiz. A
// self-check retries until right, and a right pick moves on after a green
// flash; the phone round adds "None of these" and its first pick is final.

/// One quiz item; [next] replaces it.
class _Host extends StatefulWidget {
  final int wordNumber;
  final List<String> options;
  final bool noneOfThese;
  final Future<bool?> Function(String? word, _HostState host) onPick;

  const _Host({
    required this.wordNumber,
    required this.options,
    required this.onPick,
    this.noneOfThese = false,
  });

  @override
  State<_Host> createState() => _HostState();
}

class _HostState extends State<_Host> {
  late int wordNumber = widget.wordNumber;
  late List<String> options = widget.options;
  int done = 0;

  void next(int number, List<String> next) => setState(() {
    wordNumber = number;
    options = next;
    done += 1;
  });

  void reshuffle(List<String> next) => setState(() => options = next);

  @override
  Widget build(BuildContext context) => MaterialApp(
    home: Scaffold(
      body: SingleChildScrollView(
        child: CardQuiz(
          wordNumber: wordNumber,
          options: options,
          total: 32,
          done: done,
          noneOfThese: widget.noneOfThese,
          onPick: (word) => widget.onPick(word, this),
        ),
      ),
    ),
  );
}

/// The words on the option buttons, top to bottom.
List<String> _shown(WidgetTester tester) => [
  for (final w in tester.widgetList<CardWord>(find.byType(CardWord))) w.word,
];

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  testWidgets('a wrong pick shakes and stays; a right one moves on', (
    tester,
  ) async {
    final picks = <String?>[];
    await tester.pumpWidget(
      _Host(
        wordNumber: 33,
        options: const ['SIDE', 'SIZE', 'SISTER'],
        onPick: (word, host) async {
          picks.add(word);
          if (word != 'SIZE') return false;
          // Core's next item comes while the right pick is still green.
          host.next(34, const ['CLICK', 'CLOTH', 'CLOCK']);
          return true;
        },
      ),
    );
    expect(find.text('Select word 33'), findsOneWidget);

    await tester.tap(find.byType(CardWord).at(2));
    await tester.pump();
    expect(picks, ['SISTER']);
    // While it shakes, another tap is ignored.
    await tester.tap(find.byType(CardWord).at(1));
    await tester.pump(const Duration(milliseconds: 100));
    expect(picks, ['SISTER']);
    await tester.pump(quizWrongDelay);
    expect(find.text('Select word 33'), findsOneWidget);

    await tester.tap(find.byType(CardWord).at(1));
    await tester.pump();
    expect(picks, ['SISTER', 'SIZE']);
    // The green flash holds the item a moment before the next shows.
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('Select word 33'), findsOneWidget);
    await tester.pump(quizRightDelay);
    expect(find.text('Select word 34'), findsOneWidget);
    expect(_shown(tester), ['CLICK', 'CLOTH', 'CLOCK']);
  });

  testWidgets('the same item again keeps its options in place', (tester) async {
    await tester.pumpWidget(
      _Host(
        wordNumber: 40,
        options: const ['SIDE', 'SIZE', 'SISTER'],
        onPick: (word, host) async {
          // Core shuffles again when it asks the same word after a miss.
          host.reshuffle(const ['SIZE', 'SISTER', 'SIDE']);
          return false;
        },
      ),
    );
    await tester.tap(find.byType(CardWord).first);
    await tester.pump();
    await tester.pump(quizWrongDelay);
    await tester.pump();
    expect(_shown(tester), ['SIDE', 'SIZE', 'SISTER']);
  });

  testWidgets('"None of these": the first pick is final', (tester) async {
    final picks = <String?>[];
    await tester.pumpWidget(
      _Host(
        wordNumber: 7,
        options: const ['SIZE', 'SIDE', 'SISTER'],
        noneOfThese: true,
        onPick: (word, host) async {
          picks.add(word);
          return null;
        },
      ),
    );
    expect(find.text(noneOfTheseLabel), findsOneWidget);
    await tester.tap(find.text(noneOfTheseLabel));
    await tester.pump(const Duration(seconds: 2));
    expect(picks, [null]);
    // Nothing more goes out: not another option, not None again.
    await tester.tap(find.byType(CardWord).first);
    await tester.tap(find.text(noneOfTheseLabel));
    await tester.pump(const Duration(seconds: 2));
    expect(picks, [null]);
  });

  testWidgets('an error leaves the item to pick again', (tester) async {
    var calls = 0;
    await tester.pumpWidget(
      _Host(
        wordNumber: 50,
        options: const ['SIZE', 'SIDE', 'SISTER'],
        onPick: (word, host) async {
          calls += 1;
          throw StateError('unplugged');
        },
      ),
    );
    await tester.tap(find.byType(CardWord).first);
    await tester.pump();
    await tester.tap(find.byType(CardWord).first);
    await tester.pump();
    expect(calls, 2);
  });

  testWidgets('the progress bar has a segment per item', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: QuizProgressBar(total: 37, done: 5)),
      ),
    );
    expect(find.bySemanticsLabel('5 of 37 words checked'), findsOneWidget);
  });
}
