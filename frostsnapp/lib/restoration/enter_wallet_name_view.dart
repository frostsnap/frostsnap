import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/name.dart';

class EnterWalletNameView extends StatefulWidget with TitledWidget {
  final String? initialWalletName;
  final BitcoinNetwork? initialBitcoinNetwork;
  final Function(String walletName, BitcoinNetwork network) onWalletNameEntered;

  const EnterWalletNameView({
    super.key,
    required this.onWalletNameEntered,
    this.initialWalletName,
    this.initialBitcoinNetwork,
  });

  @override
  State<EnterWalletNameView> createState() => _EnterWalletNameViewState();

  @override
  String get titleText => 'Wallet name';
}

class _EnterWalletNameViewState extends State<EnterWalletNameView> {
  final _formKey = GlobalKey<FormState>();
  final _walletNameController = TextEditingController();
  BitcoinNetwork bitcoinNetwork = BitcoinNetwork.bitcoin;
  bool _isButtonEnabled = false;

  @override
  void initState() {
    super.initState();
    _walletNameController.addListener(_updateButtonState);
    final initialWalletName = widget.initialWalletName;
    if (initialWalletName != null) {
      _walletNameController.text = initialWalletName;
    }
    final initialBitcoinNetwork = widget.initialBitcoinNetwork;
    if (initialBitcoinNetwork != null) {
      bitcoinNetwork = initialBitcoinNetwork;
    }
  }

  void _updateButtonState() {
    setState(() {
      _isButtonEnabled = _walletNameController.text.isNotEmpty;
    });
  }

  void _submitForm() {
    if (_isButtonEnabled && _formKey.currentState!.validate()) {
      widget.onWalletNameEntered(
        _walletNameController.text.trim(),
        bitcoinNetwork,
      );
    }
  }

  @override
  void dispose() {
    _walletNameController.removeListener(_updateButtonState);
    _walletNameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final developerMode = SettingsContext.of(
      context,
    )!.settings.isInDeveloperMode();

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Enter the wallet name from your physical backup.\nIf it\'s missing or unreadable, choose another name — this won\'t affect your wallet\'s security.",
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 24),
          TextFormField(
            controller: _walletNameController,
            maxLength: keyNameMaxLength(),
            inputFormatters: [nameInputFormatter],
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Wallet Name',
              border: OutlineInputBorder(),
              hintText: 'The name of the wallet being restored',
            ),
            onChanged: (_) => _updateButtonState(),
            onFieldSubmitted: (_) => _submitForm(),
            validator: (value) {
              if (value == null || value.isEmpty) {
                return 'Please enter a wallet name';
              }
              return null;
            },
          ),
          if (developerMode) ...[
            SizedBox(height: 16),
            BitcoinNetworkChooser(
              value: bitcoinNetwork,
              onChanged: (network) {
                setState(() => bitcoinNetwork = network);
              },
            ),
          ],
          const SizedBox(height: 24),
          Align(
            alignment: AlignmentDirectional.centerEnd,
            child: FilledButton(
              child: const Text('Continue'),
              onPressed: _isButtonEnabled ? _submitForm : null,
            ),
          ),
        ],
      ),
    );
  }
}
