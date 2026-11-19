import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';
import 'package:frostsnap/verifiable_card/hold_button.dart';

// Words 33–64 as the phone shows them for writing (design §5.2 P3 to P5):
// the device's backup display on the phone. First the confirm that words
// 1–32 are written (owner B1), then three words a page, the number small and
// the word large; then all 32 in two columns; then the hold. No word reaches
// the semantics tree, and nothing here can be selected or typed into.

/// The first word of the phone's half.
const firstPhoneWord = 33;
const lastPhoneWord = 64;

/// "Have you written words 1–32?", once the device's quiz on them passed
/// (owner B1). The hold tells the device, which then starts checking the
/// card; only then does this phone show words 33–64.
class DeviceHalfConfirmPage extends StatelessWidget {
  final VoidCallback? onConfirmed;

  /// Owner B15: the card is written unverified, so the device checks nothing
  /// after the confirm.
  final bool unverified;

  const DeviceHalfConfirmPage({
    super.key,
    required this.onConfirmed,
    this.unverified = false,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return CardFlowPage(
      leading: ImageIcon(
        const AssetImage('assets/icons/device2.png'),
        size: 64,
        color: theme.colorScheme.primary,
      ),
      title: 'Have you written words 1–32?',
      children: [
        Text(
          'Your device says its quiz passed. Check that each of words 1–32 is '
          'next to its number on your card.',
          style: theme.textTheme.bodyLarge,
        ),
        Text(
          unverified
              ? "Once you confirm, your device won't show words 1–32 again. "
                    'This phone then shows words 33–64.'
              : "Once you confirm, your device checks your card and won't "
                    'show words 1–32 again. This phone then shows words 33–64.',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 8),
        Center(
          child: HoldToConfirmButton(
            label: const Text(
              "I've written words 1–32",
              textAlign: TextAlign.center,
            ),
            onComplete: onConfirmed,
          ),
        ),
        Text(
          'Hold to confirm.',
          textAlign: TextAlign.center,
          style: theme.textTheme.bodyMedium,
        ),
      ],
    );
  }
}

/// One word to write: "33." in small secondary text, then the word.
class CardWordRow extends StatelessWidget {
  final int number;
  final String word;
  final double fontSize;

  const CardWordRow({
    super.key,
    required this.number,
    required this.word,
    this.fontSize = 34,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.baseline,
      textBaseline: TextBaseline.alphabetic,
      children: [
        SizedBox(
          width: fontSize * 1.6,
          child: Text(
            '$number.',
            style: theme.textTheme.titleMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        Expanded(
          child: ExcludeSemantics(
            child: Text(
              word.toUpperCase(),
              style: monospaceTextStyle.copyWith(
                fontSize: fontSize,
                fontWeight: FontWeight.w600,
                color: theme.colorScheme.onSurface,
                letterSpacing: 1.5,
              ),
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.visible,
            ),
          ),
        ),
      ],
    );
  }
}

/// Back and Next under a page of words. A null callback is off.
class CardPageNav extends StatelessWidget {
  final VoidCallback? onBack;
  final VoidCallback? onNext;
  final String nextLabel;

  const CardPageNav({
    super.key,
    required this.onBack,
    required this.onNext,
    this.nextLabel = 'Next',
  });

  @override
  Widget build(BuildContext context) => Row(
    spacing: 12,
    children: [
      Expanded(
        child: OutlinedButton(
          onPressed: onBack,
          style: OutlinedButton.styleFrom(
            minimumSize: const Size.fromHeight(48),
          ),
          child: const Text('Back'),
        ),
      ),
      Expanded(
        child: FilledButton(
          onPressed: onNext,
          style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
          child: Text(nextLabel),
        ),
      ),
    ],
  );
}

/// P3: three words of 33–64 (the last page two), from [firstWord].
class PhoneWordsPage extends StatelessWidget {
  final int page;
  final int firstWord;
  final List<String> words;
  final VoidCallback? onBack;
  final VoidCallback? onNext;

  const PhoneWordsPage({
    super.key,
    required this.page,
    required this.firstWord,
    required this.words,
    required this.onBack,
    required this.onNext,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return CardFlowPage(
      title: 'Write words $firstPhoneWord–$lastPhoneWord',
      children: [
        Text(
          'Write each word next to its number on your card.',
          style: theme.textTheme.bodyLarge,
        ),
        const SizedBox(height: 8),
        for (var i = 0; i < words.length; i++)
          CardWordRow(number: firstWord + i, word: words[i]),
        const SizedBox(height: 8),
        CardPageNav(onBack: page == 0 ? null : onBack, onNext: onNext),
      ],
    );
  }
}

/// P4: words 33–64 in two columns, 33–48 and 49–64.
class PhoneAllWordsPage extends StatelessWidget {
  final List<String> words;
  final VoidCallback? onBack;
  final VoidCallback? onNext;

  const PhoneAllWordsPage({
    super.key,
    required this.words,
    required this.onBack,
    required this.onNext,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final half = (words.length + 1) ~/ 2;
    Widget column(int from, int to) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 2,
        children: [
          for (var i = from; i < to; i++)
            CardWordRow(
              number: firstPhoneWord + i,
              word: words[i],
              fontSize: 15,
            ),
        ],
      ),
    );
    return CardFlowPage(
      title: 'All words $firstPhoneWord–$lastPhoneWord',
      children: [
        Text(
          'Check your card against these.',
          style: theme.textTheme.bodyLarge,
        ),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          spacing: 16,
          children: [column(0, half), column(half, words.length)],
        ),
        CardPageNav(onBack: onBack, onNext: onNext),
      ],
    );
  }
}

/// P5: the standard "Verify you've recorded" page, then the hold.
class PhoneConfirmPage extends StatelessWidget {
  final VoidCallback? onBack;
  final VoidCallback? onConfirmed;

  const PhoneConfirmPage({
    super.key,
    required this.onBack,
    required this.onConfirmed,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return CardFlowPage(
      title: "Check you've written",
      children: [
        Text(
          'Words $firstPhoneWord–$lastPhoneWord, each next to its number.',
          style: theme.textTheme.bodyLarge,
        ),
        Text(
          'Your device showed words 1–32.',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 8),
        Center(
          child: HoldToConfirmButton(
            label: const Text('Hold to confirm', textAlign: TextAlign.center),
            onComplete: onConfirmed,
          ),
        ),
        Text(
          "Next, a quick quiz on the words you've written.",
          textAlign: TextAlign.center,
          style: theme.textTheme.bodyMedium,
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton(onPressed: onBack, child: const Text('Back')),
        ),
      ],
    );
  }
}
