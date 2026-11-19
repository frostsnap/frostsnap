import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';

/// How many leading letters the final check puts in bold. No two BIP39 words
/// share their first four letters, so a substituted lookalike can never match
/// all four (spec L1.7.4).
const cardWordBoldLetters = 4;

/// A card word as the phone round shows it: capitals, as written on the card,
/// with the first four letters bold.
class CardWord extends StatelessWidget {
  final String word;
  final double fontSize;

  const CardWord(this.word, {super.key, this.fontSize = 40});

  /// The word split for display: the bold head and the regular tail. A word of
  /// four letters or fewer is all head.
  static (String, String) split(String word) {
    final upper = word.toUpperCase();
    final cut = upper.length < cardWordBoldLetters
        ? upper.length
        : cardWordBoldLetters;
    return (upper.substring(0, cut), upper.substring(cut));
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (head, tail) = split(word);
    final base = monospaceTextStyle.copyWith(
      fontSize: fontSize,
      color: theme.colorScheme.onSurface,
      fontWeight: FontWeight.w300,
      letterSpacing: 2,
    );
    return Text.rich(
      TextSpan(
        style: base,
        children: [
          TextSpan(
            text: head,
            style: const TextStyle(fontWeight: FontWeight.w800),
          ),
          TextSpan(text: tail),
        ],
      ),
      textAlign: TextAlign.center,
    );
  }
}

/// The word spelled out letter by letter, "S-I-Z-E", for the spelling confirm
/// and the try-again banner. The hyphens are non-breaking (U+2011), so a line
/// never wraps inside the spelled word.
String spellCardWord(String word) =>
    word.toUpperCase().split('').join('\u2011');
