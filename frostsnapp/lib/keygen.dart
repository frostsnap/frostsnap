import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/theme.dart';

void showWalletCreatedDialog(
  BuildContext context,
  AccessStructure accessStructure,
) async {
  await showDialog(
    context: context,
    barrierDismissible: false,
    builder: (BuildContext context) {
      return BackdropFilter(
        filter: blurFilter,
        child: AlertDialog(
          title: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: const [
              Text(
                'Wallet created!\nNow let\'s secure it.',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
              ),
              Icon(Icons.checklist, size: 40),
            ],
          ),
          content: const Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Before receiving any Bitcoin, you should backup and distribute your Frostsnaps.',
              ),
              SizedBox(height: 16),
              Text(
                'With each of your Frostsnaps you will:',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.directions_walk),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Travel to a location where you will store it.',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.edit),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Record the backup on the provided backup sheet (~5 mins).',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.lock),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text('Securely store the Frostsnap and its backup.'),
                  ),
                ],
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
              },
              child: const Text('Later'),
            ),
            FilledButton(
              onPressed: () {
                Navigator.of(context).pop();
                showBottomSheetOrDialog(
                  context,
                  title: Text('Backup Checklist'),
                  builder: (context, scrollController) {
                    final backupManager = FrostsnapContext.of(
                      context,
                    )!.backupManager;
                    return SuperWalletContext.of(
                      context,
                    )!.tryWrapInWalletContext(
                      keyId: accessStructure.masterAppkey().keyId(),
                      child: BackupChecklist(
                        backupManager: backupManager,
                        accessStructure: accessStructure,
                        scrollController: scrollController,
                        showAppBar: false,
                      ),
                    );
                  },
                );
              },
              child: const Text('Secure Wallet'),
            ),
          ],
        ),
      );
    },
  );
}
