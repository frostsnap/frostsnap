import 'package:flutter/material.dart';
import 'package:frostsnap/bitcoin_network_ext.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';

// The chip keeps a non-mainnet choice visible while the picker is collapsed.
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

  void _select(BitcoinNetwork network) {
    setState(() => _hidden = true);
    widget.onChanged(network);
  }

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
          style: theme.textTheme.labelMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        SegmentedButton<BitcoinNetwork>(
          showSelectedIcon: false,
          segments: BitcoinNetwork.supportedNetworks()
              .map(
                (network) => ButtonSegment(
                  value: network,
                  label: Text(
                    network.displayName,
                    overflow: TextOverflow.fade,
                    softWrap: false,
                  ),
                ),
              )
              .toList(),
          selected: {widget.selected},
          onSelectionChanged: (selected) => _select(selected.first),
        ),
        const SizedBox(height: 8),
      ],
    );
    return Column(
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
                label: Text(widget.selected.displayName),
                deleteIcon: const Icon(Icons.clear_rounded),
                onDeleted: () => _select(BitcoinNetwork.bitcoin),
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
    );
  }
}
