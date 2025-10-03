import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/src/rust/api/settings.dart' show UntrustedCertificate;

class TofuCertificateDialog extends StatelessWidget {
  final UntrustedCertificate certificateInfo;
  final String serverUrl;
  final VoidCallback onAccept;
  final VoidCallback onReject;

  const TofuCertificateDialog({
    super.key,
    required this.certificateInfo,
    required this.serverUrl,
    required this.onAccept,
    required this.onReject,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isChanged = certificateInfo.isChanged;

    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(24.0),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            // Server info
            Container(
              width: double.infinity,
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
              decoration: BoxDecoration(
                color: theme.colorScheme.surfaceContainerHighest.withValues(
                  alpha: 0.5,
                ),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Column(
                children: [
                  Text(
                    'Server',
                    style: theme.textTheme.labelMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    serverUrl,
                    style: theme.textTheme.bodyLarge?.copyWith(
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 16),

            // Certificate details
            Container(
              width: double.infinity,
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: theme.colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: isChanged
                      ? theme.colorScheme.error.withValues(alpha: 0.5)
                      : theme.colorScheme.outline.withValues(alpha: 0.3),
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Text(
                    'Certificate Fingerprint (SHA-256):',
                    style: theme.textTheme.labelMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 8),
                  SelectableText(
                    _formatFingerprint(certificateInfo.fingerprint),
                    style: monospaceTextStyle.copyWith(
                      fontSize: 13,
                      color: theme.colorScheme.onSurface,
                    ),
                    textAlign: TextAlign.center,
                  ),

                  if (isChanged && certificateInfo.oldFingerprint != null) ...[
                    const SizedBox(height: 16),
                    Text(
                      'Previous Fingerprint:',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.error,
                      ),
                    ),
                    const SizedBox(height: 8),
                    SelectableText(
                      _formatFingerprint(certificateInfo.oldFingerprint!),
                      style: monospaceTextStyle.copyWith(
                        fontSize: 13,
                        color: theme.colorScheme.error,
                        decoration: TextDecoration.lineThrough,
                      ),
                      textAlign: TextAlign.center,
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 16),

            // Show valid names warning if certificate was rejected for name mismatch
            if (certificateInfo.validForNames != null) ...[
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(16),
                decoration: BoxDecoration(
                  color: theme.colorScheme.tertiaryContainer,
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(
                    color: theme.colorScheme.tertiary.withValues(alpha: 0.5),
                  ),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(
                          Icons.warning_outlined,
                          size: 20,
                          color: theme.colorScheme.onTertiaryContainer,
                        ),
                        const SizedBox(width: 8),
                        Text(
                          'Certificate Name Mismatch',
                          style: theme.textTheme.titleSmall?.copyWith(
                            color: theme.colorScheme.onTertiaryContainer,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'You are connecting to: $serverUrl',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onTertiaryContainer,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      'But the certificate is only valid for:',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onTertiaryContainer,
                      ),
                    ),
                    const SizedBox(height: 4),
                    ...certificateInfo.validForNames!.map(
                      (name) => Padding(
                        padding: const EdgeInsets.only(left: 16, top: 2),
                        child: Text(
                          '• $name',
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onTertiaryContainer,
                            fontWeight: FontWeight.w500,
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'This could mean you are connecting to the wrong server or there is a misconfiguration.',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onTertiaryContainer,
                        fontStyle: FontStyle.italic,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 16),
            ],

            // Warning message
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: isChanged
                    ? theme.colorScheme.errorContainer
                    : theme.colorScheme.secondaryContainer,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Row(
                children: [
                  Icon(
                    Icons.info_outline,
                    size: 20,
                    color: isChanged
                        ? theme.colorScheme.onErrorContainer
                        : theme.colorScheme.onSecondaryContainer,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      isChanged
                          ? 'The certificate for this server has changed. This could indicate a security issue. Only accept if you trust this new certificate.'
                          : 'This is the first time connecting to this server. Only accept if you trust this certificate.',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: isChanged
                            ? theme.colorScheme.onErrorContainer
                            : theme.colorScheme.onSecondaryContainer,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 32),

            // Action buttons
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(onPressed: onReject, child: const Text('Reject')),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: onAccept,
                  style: isChanged
                      ? FilledButton.styleFrom(
                          backgroundColor: theme.colorScheme.error,
                          foregroundColor: theme.colorScheme.onError,
                        )
                      : null,
                  child: const Text('Trust Certificate'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  String _formatFingerprint(String fingerprint) {
    // Format raw hex fingerprint with colons for readability
    final buffer = StringBuffer();
    for (int i = 0; i < fingerprint.length; i += 2) {
      if (i > 0 && i % 16 == 0) {
        buffer.write('\n');
      } else if (i > 0) {
        buffer.write(':');
      }
      if (i + 1 < fingerprint.length) {
        buffer.write(fingerprint.substring(i, i + 2).toUpperCase());
      }
    }
    return buffer.toString();
  }
}

Future<bool?> showTofuCertificateDialog({
  required BuildContext context,
  required UntrustedCertificate certificateInfo,
  required String serverUrl,
}) async {
  final isChanged = certificateInfo.isChanged;

  final theme = Theme.of(context);

  return await showBottomSheetOrDialog<bool>(
    context,
    title: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(
          isChanged ? Icons.warning_amber_rounded : Icons.security_rounded,
          color: isChanged
              ? theme.colorScheme.error
              : theme.colorScheme.primary,
          size: 24,
        ),
        const SizedBox(width: 8),
        Text(isChanged ? 'Certificate Changed!' : 'New Certificate'),
      ],
    ),
    builder: (context, scrollController) {
      return SingleChildScrollView(
        controller: scrollController,
        child: TofuCertificateDialog(
          certificateInfo: certificateInfo,
          serverUrl: serverUrl,
          onAccept: () => Navigator.of(context).pop(true),
          onReject: () => Navigator.of(context).pop(false),
        ),
      );
    },
  );
}
