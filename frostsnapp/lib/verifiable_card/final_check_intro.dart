import 'package:dynamic_color/dynamic_color.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/verifiable_card/card_finished_view.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';

/// P8 (design §5.2): both quizzes passed, so the card is written and
/// checked; the last step is the challenge on each screen. Until the device
/// has proved its whole seal (spec PK.7 (e), owner B1) the page shows how far
/// it has come and [Continue] stays off. [onStart] is null while it is off:
/// proofs still to come, or an action on its way.
class FinalCheckIntro extends StatelessWidget {
  final int proofsVerified;
  final int sealSize;
  final VoidCallback? onStart;

  const FinalCheckIntro({
    super.key,
    required this.proofsVerified,
    required this.sealSize,
    required this.onStart,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final proven = proofsVerified >= sealSize;
    return CardFlowPage(
      leading: const AnimatedCheckCircle(size: 64),
      title: 'Success! Words 33–64 checked',
      titleColor: Colors.green.harmonizeWith(theme.colorScheme.primary),
      children: [
        flowBody(
          context,
          proven
              ? 'Your card is written and checked. Last step: 5 questions on '
                    'your device, then 5 here.'
              : 'Your device is still checking your card. Last step, once it '
                    'has: 5 questions on your device, then 5 here.',
        ),
        flowBody(
          context,
          'Each asks about a word from the other screen, to check that both '
          'screens showed you the right words.',
        ),
        if (!proven) ...[
          ProofsProgress(proofsVerified: proofsVerified, sealSize: sealSize),
          flowBody(
            context,
            'Keep your device plugged in. Continue comes on once it has '
            'checked your card.',
          ),
        ],
        FilledButton(
          onPressed: proven ? onStart : null,
          child: const Text('Continue'),
        ),
      ],
    );
  }
}
