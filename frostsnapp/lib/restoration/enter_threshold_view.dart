import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/dialog_content_with_actions.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';

class EnterThresholdView extends StatefulWidget with TitledWidget {
  final String walletName;
  final BitcoinNetwork network;
  final Function(int? threshold) onThresholdEntered;
  final int? initialThreshold;

  const EnterThresholdView({
    super.key,
    required this.walletName,
    required this.onThresholdEntered,
    required this.network,
    this.initialThreshold,
  });

  @override
  State<EnterThresholdView> createState() => _EnterThresholdViewState();

  @override
  String get titleText => 'Wallet Threshold (Optional)';
}

class _EnterThresholdViewState extends State<EnterThresholdView> {
  final _formKey = GlobalKey<FormState>();
  final _thresholdController = TextEditingController();
  final _thresholdFocusNode = FocusNode();
  int? _threshold;
  bool _specifyThreshold = false;

  @override
  void initState() {
    super.initState();
    final initialThreshold = widget.initialThreshold;
    if (initialThreshold != null) {
      _threshold = initialThreshold;
      _specifyThreshold = true;
      _thresholdController.text = initialThreshold.toString();
    }
  }

  @override
  void dispose() {
    _thresholdController.dispose();
    _thresholdFocusNode.dispose();
    super.dispose();
  }

  void _handleSubmit() {
    if (_specifyThreshold && _formKey.currentState!.validate()) {
      widget.onThresholdEntered(_threshold);
    } else if (!_specifyThreshold) {
      widget.onThresholdEntered(null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return DialogContentWithActions(
      content: Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.enter) {
            _handleSubmit();
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: Form(
          key: _formKey,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'If you know the threshold of the wallet, enter it here. Otherwise, we will determine it as we go.',
                style: theme.textTheme.bodyMedium,
              ),
              const SizedBox(height: 24),
              RadioGroup<bool>(
                groupValue: _specifyThreshold,
                onChanged: (value) {
                  setState(() {
                    _specifyThreshold = value ?? false;
                    if (!_specifyThreshold) {
                      _threshold = null;
                    }
                  });
                },
                child: Column(
                  children: [
                    Card.outlined(
                      child: InkWell(
                        onTap: () {
                          setState(() {
                            _specifyThreshold = false;
                            _threshold = null;
                          });
                        },
                        borderRadius: BorderRadius.circular(12),
                        child: Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Row(
                            children: [
                              Radio<bool>(value: false),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  "I'm not sure",
                                  style: theme.textTheme.bodyLarge,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 12),
                    Card.outlined(
                      child: InkWell(
                        onTap: () {
                          setState(() {
                            _specifyThreshold = true;
                          });
                          WidgetsBinding.instance.addPostFrameCallback((_) {
                            _thresholdFocusNode.requestFocus();
                          });
                        },
                        borderRadius: BorderRadius.circular(12),
                        child: Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Row(
                                children: [
                                  Radio<bool>(value: true),
                                  const SizedBox(width: 8),
                                  Expanded(
                                    child: Text(
                                      "I know the threshold",
                                      style: theme.textTheme.bodyLarge,
                                    ),
                                  ),
                                ],
                              ),
                              const SizedBox(height: 16),
                              TextFormField(
                                controller: _thresholdController,
                                focusNode: _thresholdFocusNode,
                                enabled: _specifyThreshold,
                                keyboardType: TextInputType.number,
                                inputFormatters: [
                                  FilteringTextInputFormatter.digitsOnly,
                                ],
                                decoration: const InputDecoration(
                                  labelText: 'Threshold',
                                  border: OutlineInputBorder(),
                                  hintText: 'Number of keys needed',
                                ),
                                validator: (value) {
                                  if (!_specifyThreshold) return null;
                                  if (value == null || value.isEmpty) {
                                    return 'Please enter a threshold';
                                  }
                                  final threshold = int.tryParse(value);
                                  if (threshold == null || threshold < 1) {
                                    return 'Threshold must be at least 1';
                                  }
                                  return null;
                                },
                                onChanged: (value) {
                                  setState(() {
                                    _threshold = int.tryParse(value);
                                  });
                                },
                                onFieldSubmitted: (_) => _handleSubmit(),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
      actions: [
        FilledButton(child: const Text('Continue'), onPressed: _handleSubmit),
      ],
    );
  }
}
