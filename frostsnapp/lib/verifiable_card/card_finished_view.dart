import 'package:flutter/material.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';

/// How far the device's proofs have come, as the phone verifies them: a line
/// and a bar, never a count (spec PK.7 (d)).
class ProofsProgress extends StatelessWidget {
  final int proofsVerified;

  /// All the proofs: the seal's size, `card_seal_size` on the bridge.
  final int sealSize;

  /// The device is plugged in. Unplugged, its proofs wait for it.
  final bool connected;

  const ProofsProgress({
    super.key,
    required this.proofsVerified,
    required this.sealSize,
    this.connected = true,
  });

  bool get done => proofsVerified >= sealSize;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final line = done
        ? 'Your device has checked your card.'
        : connected
        ? 'Your device is checking your card.'
        : 'Your device checks your card once it is plugged back in.';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        Text(
          line,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        LinearProgressIndicator(
          value: sealSize == 0 ? null : (proofsVerified / sealSize).clamp(0, 1),
        ),
      ],
    );
  }
}

/// "Checking your card", while the device proves its seal and no page of
/// words is up: a resumed flow, before its page of words comes back.
class CardProvingView extends StatelessWidget {
  final int proofsVerified;
  final int sealSize;

  const CardProvingView({
    super.key,
    required this.proofsVerified,
    required this.sealSize,
  });

  @override
  Widget build(BuildContext context) => CardFlowPage(
    title: 'Checking your card',
    children: [
      flowBody(
        context,
        'This takes about a minute and a half. Keep your device plugged in.',
      ),
      ProofsProgress(proofsVerified: proofsVerified, sealSize: sealSize),
    ],
  );
}
