import 'package:flutter/material.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

const _months = [
  'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', //
  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
];

String formatProvisionedAt(int unixSeconds) {
  final at = DateTime.fromMillisecondsSinceEpoch(
    unixSeconds * 1000,
    isUtc: true,
  );
  String two(int n) => n.toString().padLeft(2, '0');
  return '${at.day} ${_months[at.month - 1]} ${at.year}, '
      '${two(at.hour)}:${two(at.minute)} UTC';
}

/// What the factory certified about a genuine device, laid out as label/value rows.
class CertificateDetails extends StatelessWidget {
  const CertificateDetails({super.key, required this.certificate});

  final GenuineCertificate certificate;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    final labelStyle = theme.textTheme.bodyMedium?.copyWith(
      color: cs.onSurfaceVariant,
    );
    final valueStyle = theme.textTheme.bodyMedium?.copyWith(
      color: cs.onSurface,
    );

    TableRow row(String label, Widget value) => TableRow(
      children: [
        Padding(
          padding: const EdgeInsets.only(right: 16, top: 6, bottom: 6),
          child: Text(label, style: labelStyle),
        ),
        Padding(padding: const EdgeInsets.symmetric(vertical: 6), child: value),
      ],
    );

    final caseColor = certificate.caseColor;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: BoxDecoration(
        color: cs.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Table(
        columnWidths: const {0: IntrinsicColumnWidth(), 1: FlexColumnWidth()},
        defaultVerticalAlignment: TableCellVerticalAlignment.middle,
        children: [
          row(
            'Serial',
            Text(
              certificate.serial,
              style: monospaceTextStyle.merge(valueStyle),
            ),
          ),
          row('Revision', Text(certificate.revision, style: valueStyle)),
          if (caseColor != null)
            row(
              'Colour',
              Row(
                children: [
                  Container(
                    width: 12,
                    height: 12,
                    decoration: BoxDecoration(
                      color: caseColor.color,
                      shape: BoxShape.circle,
                      border: Border.all(color: cs.outline, width: 1),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text(caseColor.label, style: valueStyle),
                ],
              ),
            ),
          row(
            'Provisioned',
            Text(
              formatProvisionedAt(certificate.provisionedAt),
              style: valueStyle,
            ),
          ),
        ],
      ),
    );
  }
}
