import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';

/// Minimal nonce replenishment widget for use in wallet creation flow.
/// Shows aggregate progress with auto-advance capability.
class MinimalNonceReplenishWidget extends StatelessWidget {
  final Stream<NonceReplenishState> stream;
  final VoidCallback? onComplete;
  final VoidCallback? onAbort;
  final bool autoAdvance;

  const MinimalNonceReplenishWidget({
    super.key,
    required this.stream,
    this.onComplete,
    this.onAbort,
    this.autoAdvance = false,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return StreamBuilder<NonceReplenishState>(
      stream: stream,
      builder: (context, snapshot) {
        final state = snapshot.data;

        Widget content;
        if (state == null) {
          content = Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 24),
              Text('Connecting...', style: theme.textTheme.bodyLarge),
            ],
          );
        } else {
          // Handle abort (device disconnection)
          if (state.abort && onAbort != null) {
            WidgetsBinding.instance.addPostFrameCallback((_) {
              onAbort!();
            });
          }

          final progress = state.receivedFrom.length;
          final total = state.devices.length;
          final isComplete = progress == total;

          // Auto-advance when complete
          if (isComplete && autoAdvance && onComplete != null) {
            WidgetsBinding.instance.addPostFrameCallback((_) {
              onComplete!();
            });
          }

          content = Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('Preparing devices', style: theme.textTheme.headlineMedium),
              SizedBox(height: 32),
              Stack(
                alignment: Alignment.center,
                children: [
                  SizedBox(
                    width: 120,
                    height: 120,
                    child: CircularProgressIndicator(
                      value: total > 0 ? progress / total : null,
                      strokeWidth: 8,
                      backgroundColor:
                          theme.colorScheme.surfaceContainerHighest,
                    ),
                  ),
                  Text(
                    '$progress of $total',
                    style: theme.textTheme.headlineSmall,
                  ),
                ],
              ),
              SizedBox(height: 16),
              Text(
                isComplete ? 'Complete!' : 'Please wait...',
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: isComplete
                      ? theme.colorScheme.primary
                      : theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          );
        }

        return Padding(
          padding: EdgeInsets.symmetric(vertical: 32),
          child: Align(alignment: Alignment.topCenter, child: content),
        );
      },
    );
  }
}

/// Full-screen nonce replenishment dialog with Done/Cancel buttons.
/// Used after wallet restoration where user interaction is required.
class NonceReplenishDialog extends StatelessWidget {
  final Stream<NonceReplenishState> stream;
  final VoidCallback? onCancel;

  const NonceReplenishDialog({super.key, required this.stream, this.onCancel});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mediaQuery = MediaQuery.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: CustomScrollView(
            physics: ClampingScrollPhysics(),
            shrinkWrap: true,
            slivers: [
              SliverAppBar(
                title: Text(
                  'Preparing devices',
                  style: theme.textTheme.titleMedium,
                ),
                automaticallyImplyLeading: false,
                pinned: true,
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(20, 20, 20, 28),
                sliver: SliverToBoxAdapter(
                  child: MinimalNonceReplenishWidget(
                    stream: stream,
                    autoAdvance: false,
                  ),
                ),
              ),
              SliverPadding(padding: EdgeInsets.only(bottom: 32)),
            ],
          ),
        ),
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Divider(height: 0),
            Padding(
              padding: EdgeInsets.all(
                20,
              ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
              child: SafeArea(
                top: false,
                child: StreamBuilder<NonceReplenishState>(
                  stream: stream,
                  builder: (context, snapshot) {
                    final state = snapshot.data;
                    final allComplete =
                        state != null &&
                        state.receivedFrom.length == state.devices.length;
                    final isAborted = state?.abort ?? false;

                    return Row(
                      mainAxisSize: MainAxisSize.max,
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        Flexible(
                          child: TextButton(
                            onPressed: (!allComplete && !isAborted)
                                ? onCancel
                                : null,
                            child: Text(
                              'Cancel',
                              softWrap: false,
                              overflow: TextOverflow.fade,
                            ),
                          ),
                        ),
                        Expanded(
                          flex: 2,
                          child: Align(
                            alignment: AlignmentDirectional.centerEnd,
                            child: FilledButton(
                              onPressed: allComplete
                                  ? () => Navigator.pop(context, true)
                                  : null,
                              child: Text(
                                allComplete ? 'Done' : 'Please wait...',
                                softWrap: false,
                                overflow: TextOverflow.fade,
                              ),
                            ),
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

// Backwards compatibility alias
typedef NonceReplenishWidget = NonceReplenishDialog;
