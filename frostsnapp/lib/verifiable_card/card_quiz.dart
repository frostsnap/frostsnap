import 'dart:async';
import 'dart:math' as math;

import 'package:dynamic_color/dynamic_color.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/verifiable_card/card_word.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';

/// How long a right pick shows green, and a wrong one red and shaking, as on
/// the device's quiz (frostsnap_widgets backup/check_backup.rs).
const quizRightDelay = Duration(milliseconds: 400);
const quizWrongDelay = Duration(milliseconds: 600);

/// The label "None of these" has, on screen and for a screen reader.
const noneOfTheseLabel = 'None of these';

/// One quiz item, the standard check-backup quiz on the phone (design §5.2 P6,
/// P11, P15): "Select word N", a progress bar, and the options as big
/// buttons. With [noneOfThese] a fourth button, "None of these", follows.
///
/// [onPick] gets the word picked, or null for None of these, and says
/// whether it was right: then the button turns green and the next item
/// follows after [quizRightDelay], or it turns red and shakes for
/// [quizWrongDelay] and the same item stays. A null result gives no
/// feedback (the phone round's first pick is final): the pick stays marked
/// and the buttons off until the page moves on.
///
/// The item on screen holds still while its feedback runs: a new item given
/// meanwhile shows after it. An item given again with the same options keeps
/// their order. No word reaches the semantics tree: each button is
/// "Option 1", "Option 2", ... for a screen reader.
class CardQuiz extends StatefulWidget {
  final int wordNumber;
  final List<String> options;

  /// The progress bar's segments, and how many are done. Without [progress]
  /// the bar is drawn; with it, [progress] is drawn instead (the five dots).
  final int total;
  final int done;
  final Widget? progress;
  final bool noneOfThese;

  /// Shown above the word number, such as a second look's line.
  final String? banner;
  final Future<bool?> Function(String? word)? onPick;

  const CardQuiz({
    super.key,
    required this.wordNumber,
    required this.options,
    required this.total,
    required this.done,
    this.progress,
    this.noneOfThese = false,
    this.banner,
    required this.onPick,
  });

  @override
  State<CardQuiz> createState() => _CardQuizState();
}

enum _Feedback { none, waiting, right, wrong }

class _CardQuizState extends State<CardQuiz>
    with SingleTickerProviderStateMixin {
  int? _wordNumber;
  var _options = const <String>[];
  late int _done;

  /// The option picked (its place in [_options]), -1 for None of these.
  int? _picked;
  var _feedback = _Feedback.none;
  late final AnimationController _shake;

  @override
  void initState() {
    super.initState();
    _shake = AnimationController(vsync: this, duration: quizWrongDelay);
    _take(widget);
  }

  @override
  void dispose() {
    _shake.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(CardQuiz oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_feedback == _Feedback.none) setState(() => _take(widget));
  }

  /// Shows [quiz]'s item, keeping the order of the options on screen when
  /// it is the same item again.
  void _take(CardQuiz quiz) {
    final same =
        _wordNumber == quiz.wordNumber &&
        _options.length == quiz.options.length &&
        quiz.options.every(_options.contains);
    _wordNumber = quiz.wordNumber;
    if (!same) _options = List.of(quiz.options);
    _done = quiz.done;
  }

  Future<void> _pick(int index) async {
    final onPick = widget.onPick;
    if (onPick == null || _feedback != _Feedback.none) return;
    setState(() {
      _picked = index;
      _feedback = _Feedback.waiting;
    });
    bool? right;
    try {
      right = await onPick(index < 0 ? null : _options[index]);
    } catch (_) {
      // The page shows the error; the item stands.
      if (mounted) setState(_reset);
      return;
    }
    if (!mounted) return;
    switch (right) {
      case true:
        setState(() {
          _feedback = _Feedback.right;
          _done = math.min(_done + 1, widget.total);
        });
        await Future<void>.delayed(quizRightDelay);
      case false:
        setState(() => _feedback = _Feedback.wrong);
        await _shake.forward(from: 0);
      case null:
        return;
    }
    if (mounted) setState(_reset);
  }

  void _reset() {
    _picked = null;
    _feedback = _Feedback.none;
    _take(widget);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled = widget.onPick != null && _feedback == _Feedback.none;
    final banner = widget.banner;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 16,
      children: [
        if (banner != null)
          CardBanner(color: theme.colorScheme.error, text: banner),
        Text(
          'Select word $_wordNumber',
          textAlign: TextAlign.center,
          style: theme.textTheme.headlineSmall,
        ),
        widget.progress ?? QuizProgressBar(total: widget.total, done: _done),
        const SizedBox(height: 8),
        for (var i = 0; i < _options.length; i++)
          _shaking(
            i,
            _OptionButton(
              semanticLabel: 'Option ${i + 1}',
              state: _stateOf(i),
              onTap: enabled ? () => _pick(i) : null,
              child: CardWord(_options[i], fontSize: 28),
            ),
          ),
        if (widget.noneOfThese)
          _shaking(
            -1,
            _OptionButton(
              semanticLabel: noneOfTheseLabel,
              state: _stateOf(-1),
              onTap: enabled ? () => _pick(-1) : null,
              height: 52,
              child: Text(noneOfTheseLabel, style: theme.textTheme.titleMedium),
            ),
          ),
      ],
    );
  }

  _OptionState _stateOf(int index) {
    if (_picked != index) return _OptionState.idle;
    return switch (_feedback) {
      _Feedback.right => _OptionState.right,
      _Feedback.wrong => _OptionState.wrong,
      _ => _OptionState.picked,
    };
  }

  Widget _shaking(int index, Widget child) {
    if (_picked != index || _feedback != _Feedback.wrong) return child;
    return AnimatedBuilder(
      animation: _shake,
      builder: (context, child) => Transform.translate(
        offset: Offset(
          math.sin(_shake.value * math.pi * 6) * 10 * (1 - _shake.value),
          0,
        ),
        child: child,
      ),
      child: child,
    );
  }
}

enum _OptionState { idle, picked, right, wrong }

class _OptionButton extends StatelessWidget {
  final String semanticLabel;
  final _OptionState state;
  final VoidCallback? onTap;
  final double height;
  final Widget child;

  const _OptionButton({
    required this.semanticLabel,
    required this.state,
    required this.onTap,
    this.height = 68,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final green = Colors.green.harmonizeWith(scheme.primary);
    final (fill, border) = switch (state) {
      _OptionState.idle => (scheme.surfaceContainerHigh, scheme.outlineVariant),
      _OptionState.picked => (scheme.primaryContainer, scheme.primary),
      _OptionState.right => (green.withValues(alpha: 0.25), green),
      _OptionState.wrong => (scheme.errorContainer, scheme.error),
    };
    final radius = BorderRadius.circular(16);
    return Semantics(
      label: semanticLabel,
      button: true,
      enabled: onTap != null,
      excludeSemantics: true,
      onTap: onTap,
      child: Material(
        color: fill,
        shape: RoundedRectangleBorder(
          borderRadius: radius,
          side: BorderSide(color: border, width: 2),
        ),
        child: InkWell(
          borderRadius: radius,
          onTap: onTap,
          child: SizedBox(
            height: height,
            child: Center(
              child: FittedBox(fit: BoxFit.scaleDown, child: child),
            ),
          ),
        ),
      ),
    );
  }
}

/// The device quiz's progress bars: one segment per item.
class QuizProgressBar extends StatelessWidget {
  final int total;
  final int done;

  const QuizProgressBar({super.key, required this.total, required this.done});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Semantics(
      label: '$done of $total words checked',
      child: Row(
        spacing: 2,
        children: [
          for (var i = 0; i < total; i++)
            Expanded(
              child: AnimatedContainer(
                duration: Durations.short4,
                height: 6,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(3),
                  color: i < done
                      ? theme.colorScheme.primary
                      : theme.colorScheme.surfaceContainerHighest,
                ),
              ),
            ),
        ],
      ),
    );
  }
}
