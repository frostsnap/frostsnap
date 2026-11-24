import 'package:flutter/material.dart';
import 'package:frostsnap/bitcoin_network_ext.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/settings.dart';

/// Per-network chain source picker.
class ChainSourceSettingsPage extends StatelessWidget {
  const ChainSourceSettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final settings = SettingsContext.of(context)!;

    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: StreamBuilder(
        stream: settings.developerSettings,
        builder: (context, snap) {
          final developerMode = snap.data?.developerMode ?? false;

          return ListView(
            children: [
              for (final network in BitcoinNetwork.supportedNetworks())
                if (network.isMainnet() || developerMode)
                  _ChainSourceCard(network: network),
            ],
          );
        },
      ),
    );
  }
}

class _ChainSourceCard extends StatelessWidget {
  final BitcoinNetwork network;

  const _ChainSourceCard({required this.network});

  @override
  Widget build(BuildContext context) {
    final settingsCtx = SettingsContext.of(context)!;
    final settings = settingsCtx.settings;
    final selected = settings.getChainSource(network: network);

    Future<void> choose(ChainSource? source) async {
      if (source == null || source == selected) return;
      try {
        await settings.setChainSource(network: network, source: source);
        if (context.mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('Restart Frostsnap to use the new chain source'),
            ),
          );
        }
      } catch (error) {
        if (context.mounted) {
          ScaffoldMessenger.of(
            context,
          ).showSnackBar(SnackBar(content: Text('$error')));
        }
      }
    }

    return Card.outlined(
      margin: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
            child: Text(
              network.displayName,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ),
          RadioGroup<ChainSource>(
            groupValue: selected,
            onChanged: choose,
            child: const Column(
              children: [
                RadioListTile<ChainSource>(
                  value: ChainSource.electrum,
                  title: Text('Electrum server'),
                  subtitle: Text(
                    'Fast, and light on data. The server is told which addresses belong to this '
                    'wallet.',
                  ),
                ),
                RadioListTile<ChainSource>(
                  value: ChainSource.compactFilters,
                  title: Text('Compact block filters'),
                  subtitle: Text(
                    'No server learns which addresses are yours. Slower to sync and uses more '
                    'data.',
                  ),
                ),
              ],
            ),
          ),
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: Text(
              'Takes effect when Frostsnap restarts.',
              style: TextStyle(fontStyle: FontStyle.italic),
            ),
          ),
        ],
      ),
    );
  }
}
