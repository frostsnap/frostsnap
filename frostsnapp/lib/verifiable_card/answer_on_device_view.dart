import 'package:flutter/material.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';

/// P9, while the device round runs on the device (design §5.2): the phone
/// shows no word and no progress; the device asks about 5 of words 33–64.
/// Stop is in the app bar.
class AnswerOnDeviceView extends StatelessWidget {
  const AnswerOnDeviceView({super.key});

  @override
  Widget build(BuildContext context) => CardFlowPage(
    leading: ImageIcon(
      const AssetImage('assets/icons/device2.png'),
      size: 64,
      color: Theme.of(context).colorScheme.primary,
    ),
    title: 'Answer on your device',
    children: [
      flowBody(
        context,
        'Your device asks about 5 of the words you wrote from this phone. '
        'Pick the word you wrote, or None of these.',
      ),
    ],
  );
}
