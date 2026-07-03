import 'package:flutter/material.dart';
import 'package:frostsnap/choice_card.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';

enum RestoreChoice { local, remote }

/// The Restore verb's mechanism fork, phrased as WHO'S INVOLVED —
/// parallel to keygen's "Who is this for?" step. Pops with a
/// [RestoreChoice]; [onLocal]/[onRemote] override the pop for
/// widget tests.
class RestoreChooser extends StatelessWidget {
  const RestoreChooser({super.key, this.onLocal, this.onRemote});

  final VoidCallback? onLocal;
  final VoidCallback? onRemote;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: MultiStepDialogScaffold(
        stepKey: 'restoreChooser',
        title: const Text('Restore a wallet'),
        subtitle: 'Where are the keys?',
        showClose: true,
        body: SliverToBoxAdapter(
          child: Column(
            spacing: 12,
            children: [
              ChoiceCard(
                icon: Icons.usb_rounded,
                title: 'With your devices here',
                subtitle:
                    'Plug in devices holding keys, or enter seed-word '
                    'backups yourself.',
                emphasized: true,
                onTap:
                    onLocal ??
                    () => Navigator.of(context).pop(RestoreChoice.local),
              ),
              ChoiceCard(
                icon: Icons.groups_rounded,
                title: 'With others',
                subtitle:
                    'Other people hold key shares. Coordinate the recovery '
                    'together over nostr.',
                onTap:
                    onRemote ??
                    () => Navigator.of(context).pop(RestoreChoice.remote),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
