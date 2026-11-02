import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';

// Words 33–64 of a card, entered in the app (design §5.3 E2, E3): one plain
// text field per word, each checked against the backup word list. A blank
// field is a word the user can't read. The fields ask the keyboard for no
// suggestions and no learning. Nothing here reaches the semantics tree.

/// Words in the phone's half, and the first one's number.
const phoneHalfWords = 32;
const phoneHalfFirst = 33;

/// The most words a card can be missing and still be read: its parity.
const maxBlankWords = 3;

/// The backup word list's check. The app's is the bridge's; a test can give
/// its own.
class WordLookup {
  final bool Function(String word) isWord;

  const WordLookup({required this.isWord});

  static final bridge = WordLookup(isWord: (word) => bip39IsWord(word: word));
}

/// E2 and E3: the 32 fields, then [Send to device]. [onSend] gets the 32
/// words in capitals (null: left blank, can't read).
class PhoneHalfEntry extends StatefulWidget {
  final ValueChanged<List<String?>> onSend;

  /// The words to start from, such as the ones sent last (null: blank).
  final List<String?>? initial;
  final WordLookup? lookup;

  /// What [Send to device] says, such as "Send again".
  final String sendLabel;

  const PhoneHalfEntry({
    super.key,
    required this.onSend,
    this.initial,
    this.lookup,
    this.sendLabel = 'Send to device',
  });

  @override
  State<PhoneHalfEntry> createState() => PhoneHalfEntryState();
}

class PhoneHalfEntryState extends State<PhoneHalfEntry> {
  final _form = GlobalKey<FormState>();
  late final List<TextEditingController> _fields = [
    for (var i = 0; i < phoneHalfWords; i++)
      TextEditingController(text: widget.initial?[i] ?? '')
        ..addListener(_changed),
  ];

  WordLookup get _lookup => widget.lookup ?? WordLookup.bridge;

  /// The half as the bridge takes it: a word each, null for blank.
  List<String?> get words => [
    for (final field in _fields)
      field.text.trim().isEmpty ? null : field.text.trim().toUpperCase(),
  ];

  int get _blank => words.where((word) => word == null).length;

  void _changed() => setState(() {});

  String? _check(String? text) {
    final word = text?.trim() ?? '';
    if (word.isEmpty || _lookup.isWord(word)) return null;
    return 'Not a backup word';
  }

  void _send() {
    if (_form.currentState?.validate() != true) return;
    widget.onSend(words);
  }

  @override
  void dispose() {
    for (final field in _fields) {
      field.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final blank = _blank;
    // Every field shows a word of the card (design §5.2).
    return ExcludeSemantics(
      child: Form(
        key: _form,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12,
          children: [
            Text('Words 33–64', style: theme.textTheme.titleLarge),
            Text(
              restoreHalfLine,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            Text(
              "Leave a word blank only if you can't read it.",
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            for (var i = 0; i < phoneHalfWords; i++) _field(i),
            Text(
              blank > maxBlankWords
                  ? '${phoneHalfWords - blank} of 32 entered. Fill in at '
                        'least ${phoneHalfWords - maxBlankWords}.'
                  : '${phoneHalfWords - blank} of 32 entered',
              textAlign: TextAlign.center,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            FilledButton(
              key: const ValueKey('send-half'),
              onPressed: blank > maxBlankWords ? null : _send,
              child: Text(widget.sendLabel),
            ),
          ],
        ),
      ),
    );
  }

  Widget _field(int i) => TextFormField(
    key: ValueKey('word-${phoneHalfFirst + i}'),
    controller: _fields[i],
    validator: _check,
    autovalidateMode: AutovalidateMode.onUserInteraction,
    decoration: InputDecoration(
      labelText: 'Word ${phoneHalfFirst + i}',
      border: const OutlineInputBorder(),
      isDense: true,
    ),
    style: monospaceTextStyle.copyWith(
      fontSize: 18,
      fontWeight: FontWeight.w600,
    ),
    // No suggestions, autocorrect or learning: the keyboard keeps no word.
    keyboardType: TextInputType.visiblePassword,
    autocorrect: false,
    enableSuggestions: false,
    enableIMEPersonalizedLearning: false,
    textCapitalization: TextCapitalization.characters,
    inputFormatters: [FilteringTextInputFormatter.allow(RegExp('[a-zA-Z ]'))],
    textInputAction: i + 1 < phoneHalfWords
        ? TextInputAction.next
        : TextInputAction.done,
  );
}
