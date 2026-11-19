import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';

/// The safe next step an alarm offers (spec §0.6.4.4, §0.6.4.6).
String nextStepLine(NextStep step) => switch (step) {
  NextStep.setUpAgain => 'Set this wallet up again.',
  NextStep.withoutThisDevice => 'Set this wallet up again without this device.',
  NextStep.differentPhone => 'Set this wallet up again with a different phone.',
  NextStep.differentPhoneAndDevice =>
    'Set this wallet up again with a different phone and a different '
        'Frostsnap device.',
};

/// The alarm's title.
String alarmTitle(CardAlarmKind kind) => switch (kind) {
  CardAlarmKind.stoppedBeforeN => 'Check stopped.',
  _ => "Stop. Don't use this wallet.",
};

/// What happened, in plain words. The phone never says which word didn't
/// match.
String? alarmCause(CardAlarmKind kind) => switch (kind) {
  CardAlarmKind.phoneRoundFailed =>
    "The words 1–32 your device showed didn't match what it proved.",
  CardAlarmKind.deviceRoundFailed =>
    "Your device's check of words 33–64 failed: this phone may have shown "
        'you wrong words.',
  CardAlarmKind.deviceRoundFailedAfterN =>
    "Your device's check of words 33–64 failed. This phone or your device "
        'may be faulty.',
  CardAlarmKind.deviceFault =>
    "Your device sent this phone words or proofs that didn't match its seal.",
  // Owner B10: an honest device's seal never changes.
  CardAlarmKind.sealChanged =>
    "This device showed a different backup partway through. Don't trust it.",
  CardAlarmKind.stoppedBeforeN =>
    "This card is not checked. Don't use this wallet.",
  // Owner B9: an interruption the user didn't cause.
  CardAlarmKind.disconnectedAfterN =>
    "This device disconnected during the check, and you didn't disconnect "
        "it. Don't trust it.",
  // Owner R2-2: a fourth interruption, past the 3 resumes a check allows.
  CardAlarmKind.interruptedTooOften =>
    "This check was interrupted too many times. Don't trust this device.",
  _ => null,
};

/// P13: an alarm, in plain words: what happened, one next step, H and the
/// help line. The suspect mark itself shows in wallet creation, so it isn't
/// repeated here.
class AlarmView extends StatelessWidget {
  final CardAlarm alarm;
  final VoidCallback onClose;

  const AlarmView({super.key, required this.alarm, required this.onClose});

  @override
  Widget build(BuildContext context) {
    final cause = alarmCause(alarm.kind);
    return CardFlowPage(
      icon: Icons.dangerous_rounded,
      error: true,
      title: alarmTitle(alarm.kind),
      children: [
        ErrorCard(lines: [?cause, nextStepLine(alarm.nextStep)]),
        flowBody(context, lineH),
        flowBody(context, helpLine),
        FilledButton(onPressed: onClose, child: const Text('Close')),
      ],
    );
  }
}
