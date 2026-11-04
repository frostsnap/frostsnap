import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/broadcast.dart';
import 'package:frostsnap/src/rust/api/signer_selection.dart';

class SignerSelectionBuilder extends StatefulWidget {
  final SignerSelection selection;
  final WidgetBuilder builder;

  const SignerSelectionBuilder({
    super.key,
    required this.selection,
    required this.builder,
  });

  @override
  State<SignerSelectionBuilder> createState() => _SignerSelectionBuilderState();
}

class _SignerSelectionBuilderState extends State<SignerSelectionBuilder> {
  late UnitBroadcastSubscription _sub;
  late StreamSubscription<void> _listener;

  void _subscribe() {
    _sub = widget.selection.subscribe();
    _listener = _sub.start().listen((_) => setState(() {}));
  }

  void _unsubscribe() {
    _listener.cancel();
    _sub.dispose();
  }

  @override
  void initState() {
    super.initState();
    _subscribe();
  }

  @override
  void didUpdateWidget(SignerSelectionBuilder oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.selection, widget.selection)) {
      _unsubscribe();
      _subscribe();
    }
  }

  @override
  void dispose() {
    _unsubscribe();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.builder(context);
}

class DeviceSelectorList extends StatelessWidget {
  final SignerSelection selection;
  final String? title;
  final String? trailing;

  const DeviceSelectorList({
    super.key,
    required this.selection,
    this.title,
    this.trailing,
  });

  @override
  Widget build(BuildContext context) {
    return SignerSelectionBuilder(
      selection: selection,
      builder: (context) => SignerRows(
        title: title,
        trailing: trailing,
        rows: selection.rows(),
        onChanged: (id, checked) =>
            checked ? selection.select(dId: id) : selection.deselect(dId: id),
      ),
    );
  }
}

class SignerRows extends StatelessWidget {
  final String? title;
  final String? trailing;
  final List<SignerRow> rows;
  final void Function(DeviceId id, bool selected) onChanged;

  /// Overridable only because widget tests have no coordinator to follow genuine status through.
  final Widget Function(DeviceId id) badge;

  const SignerRows({
    super.key,
    this.title,
    this.trailing,
    required this.rows,
    required this.onChanged,
    this.badge = GenuineBadge.forDeviceId,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final title = this.title;
    final trailing = this.trailing;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (title != null)
          ListTile(
            dense: true,
            title: Text(title),
            trailing: trailing != null ? Text(trailing) : null,
          ),
        ...rows.map((row) {
          final why = _explain(row.refusal);
          final name = row.name ?? '<unknown>';
          return CheckboxListTile(
            value: row.selected,
            onChanged: row.selected || row.refusal == null
                ? (checked) => onChanged(row.id, checked ?? false)
                : null,
            secondary: CaseAccent(
              id: row.id,
              builder: (context, accent) => Icon(Icons.key, color: accent),
            ),
            title: Row(
              spacing: 8,
              children: [
                Expanded(
                  child: Text(
                    '#${row.shareIndex} $name',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                badge(row.id),
              ],
            ),
            subtitle: why == null
                ? null
                : Text(why, style: TextStyle(color: theme.colorScheme.error)),
          );
        }),
      ],
    );
  }

  /// Reaching the threshold greys a row without a reason: the user has simply picked enough.
  static String? _explain(SignerRefusal? refusal) => switch (refusal) {
    SignerRefusal_TooFewNonces(:final available, :final needed) =>
      'has nonces for $available, this needs $needed',
    SignerRefusal_ShareAlreadySelected(:final shareIndex) =>
      '#$shareIndex is already selected',
    _ => null,
  };
}
