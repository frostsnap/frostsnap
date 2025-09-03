import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/progress_indicator.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/theme.dart';

// Standalone widget for nonce replenshment to be called after recovery, this is separate from the
// streamlined widget that sits inside the wallet creation workflow.
class NonceReplenishWidget extends StatelessWidget {
  final Stream<NonceReplenishState> stream;
  final VoidCallback? onCancel;

  const NonceReplenishWidget({super.key, required this.stream, this.onCancel});

  Widget _buildDeviceCard(
    BuildContext context,
    DeviceId deviceId,
    bool isComplete,
  ) {
    final theme = Theme.of(context);
    final deviceName = coord.getDeviceName(id: deviceId);
    return Card.filled(
      margin: EdgeInsets.symmetric(vertical: 4),
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        title: Text(deviceName ?? 'Setting name..', style: monospaceTextStyle),
        leading: Icon(Icons.key),
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        trailing: Row(
          mainAxisSize: MainAxisSize.min,
          spacing: 8,
          children: [
            Flexible(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(
                    isComplete ? 'Ready' : 'Preparing..',
                    style: theme.textTheme.titleSmall?.copyWith(
                      color: isComplete ? Colors.green : null,
                    ),
                  ),
                ],
              ),
            ),
            if (isComplete)
              Icon(Icons.check_circle_rounded, color: Colors.green)
            else
              SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
          ],
        ),
      ),
    );
  }

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
                title: Text('', style: theme.textTheme.titleMedium),
                automaticallyImplyLeading: false,
                pinned: true,
              ),
              SliverToBoxAdapter(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(20, 36, 20, 36),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    spacing: 12,
                    children: [
                      Text(
                        'Preparing devices',
                        style: theme.textTheme.headlineLarge,
                      ),
                    ],
                  ),
                ),
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(20, 20, 20, 28),
                sliver: SliverToBoxAdapter(
                  child: StreamBuilder<NonceReplenishState>(
                    stream: stream,
                    builder: (context, snapshot) {
                      if (!snapshot.hasData) {
                        return Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            SizedBox(height: 20),
                            FsProgressIndicator(),
                            SizedBox(height: 20),
                            Text(
                              'Please wait...',
                              style: theme.textTheme.bodyLarge,
                              textAlign: TextAlign.center,
                            ),
                          ],
                        );
                      }

                      final state = snapshot.data!;

                      if (state.abort) {
                        return Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            SizedBox(height: 20),
                            Icon(
                              Icons.error_outline,
                              size: 48,
                              color: theme.colorScheme.error,
                            ),
                            SizedBox(height: 16),
                            Text(
                              'Process was cancelled',
                              style: theme.textTheme.bodyLarge,
                              textAlign: TextAlign.center,
                            ),
                          ],
                        );
                      }

                      final allComplete =
                          state.receivedFrom.length == state.devices.length;

                      return Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          // Device list
                          ...state.devices.map((device) {
                            final isComplete = state.receivedFrom.any(
                              (id) => deviceIdEquals(id, device),
                            );
                            return _buildDeviceCard(
                              context,
                              device,
                              isComplete,
                            );
                          }).toList(),

                          SizedBox(height: 20),

                          // Bottom status message
                          Center(
                            child: Text(
                              allComplete
                                  ? 'Devices ready.'
                                  : 'Please wait...',
                              style: theme.textTheme.bodyLarge?.copyWith(
                                fontWeight: allComplete
                                    ? FontWeight.w600
                                    : FontWeight.normal,
                              ),
                              textAlign: TextAlign.center,
                            ),
                          ),
                        ],
                      );
                    },
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
