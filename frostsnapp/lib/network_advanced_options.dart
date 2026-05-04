import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';

/// Dev-mode network picker. Collapsed by default, shows a muted
/// "Developer" text button with a dropdown chevron; expanding reveals
/// a segmented button of [BitcoinNetwork.supportedNetworks]. A summary
/// `InputChip` appears when a non-mainnet network is selected.
///
/// Shared between local keygen (`wallet_create.dart`) and remote
/// keygen (`org_keygen_page.dart`). The caller owns the selected
/// state — pass `selected` in and handle `onChanged` to persist it.
class NetworkAdvancedOptions extends StatefulWidget {
  const NetworkAdvancedOptions({
    super.key,
    required this.selected,
    required this.onChanged,
  });

  final BitcoinNetwork selected;
  final ValueChanged<BitcoinNetwork> onChanged;

  @override
  State<NetworkAdvancedOptions> createState() => _NetworkAdvancedOptionsState();
}

class _NetworkAdvancedOptionsState extends State<NetworkAdvancedOptions> {
  bool _hidden = true;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mayHide = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 12,
      children: [
        Text(
          'Network',
          style: theme.textTheme.labelMedium
              ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
        ),
        SegmentedButton<String>(
          showSelectedIcon: false,
          segments: BitcoinNetwork.supportedNetworks()
              .map(
                (network) => ButtonSegment(
                  value: network.name(),
                  label: Text(
                    network.name(),
                    overflow: TextOverflow.fade,
                    softWrap: false,
                  ),
                ),
              )
              .toList(),
          selected: {widget.selected.name()},
          onSelectionChanged: (selectedSet) {
            setState(() => _hidden = true);
            final name = selectedSet.first;
            widget.onChanged(BitcoinNetwork.fromString(string: name)!);
          },
        ),
        const SizedBox(height: 8),
      ],
    );
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16).copyWith(top: 12),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AnimatedCrossFade(
            firstChild: const SizedBox(),
            secondChild: mayHide,
            crossFadeState: _hidden
                ? CrossFadeState.showFirst
                : CrossFadeState.showSecond,
            duration: Durations.medium2,
            sizeCurve: Curves.easeInOutCubicEmphasized,
          ),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            spacing: 8,
            children: [
              if (!widget.selected.isMainnet())
                InputChip(
                  surfaceTintColor: theme.colorScheme.error,
                  label: Text(widget.selected.name()),
                  deleteIcon: const Icon(Icons.clear_rounded),
                  onDeleted: () {
                    setState(() => _hidden = true);
                    widget.onChanged(BitcoinNetwork.bitcoin);
                  },
                ),
              TextButton.icon(
                onPressed: () => setState(() => _hidden = !_hidden),
                icon: Icon(
                  _hidden
                      ? Icons.arrow_drop_up_rounded
                      : Icons.arrow_drop_down_rounded,
                ),
                label: const Text(
                  'Developer',
                  overflow: TextOverflow.fade,
                  softWrap: false,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
