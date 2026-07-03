import 'package:flutter/material.dart';

/// Icon + title + subtitle tap card for intent-fork steps ("Who is
/// this for?", "Restore a wallet"). Extracted from `OrgKeygenPage`
/// so every mechanism chooser reads as one system. Mark the common
/// path [emphasized].
class ChoiceCard extends StatelessWidget {
  const ChoiceCard({
    super.key,
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
    this.emphasized = false,
  });

  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;
  final bool emphasized;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      elevation: emphasized ? 2 : 0,
      color: emphasized
          ? theme.colorScheme.secondaryContainer
          : theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(
                icon,
                size: 32,
                color: emphasized
                    ? theme.colorScheme.onSecondaryContainer
                    : theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  spacing: 4,
                  children: [
                    Text(
                      title,
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: emphasized
                            ? theme.colorScheme.onSecondaryContainer
                            : null,
                      ),
                    ),
                    Text(
                      subtitle,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: emphasized
                            ? theme.colorScheme.onSecondaryContainer.withValues(
                                alpha: 0.8,
                              )
                            : theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              Icon(
                Icons.chevron_right_rounded,
                color: emphasized
                    ? theme.colorScheme.onSecondaryContainer
                    : theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
