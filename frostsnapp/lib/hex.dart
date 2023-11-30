import 'dart:typed_data';
import 'dart:math' as math;

import 'package:flutter/material.dart';

String toHex(Uint8List data) {
  return data.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join('');
}

// Widget toHexBox(Uint8List bytes, Orientation orientation) {
//   var row = Row();
//   var col = Column();
//   final lineLength = orientation == Orientation.portrait ? 8 : 16;
//   var item = StringBuffer();
//   for (var i = 0; i < bytes.length; i++) {
//     item.write(bytes[i].toRadixString(16).padLeft(2, '0').toUpperCase());
//     if ((i + 1) % lineLength == 0) {
//       col.children.add(row);
//       row = Row();
//     } else if ((i + 1) % 2 == 0) {
//       row.children.add(Text(item.toString()));
//       item = StringBuffer();
//     }
//   }
//   return C
// }
//
Widget toHexBox(Uint8List bytes, {int chunkSize = 2}) {
  String hexString =
      bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();

  // Function to split the hex string into chunks
  List<String> splitIntoChunks(String str, int chunkSize) {
    List<String> chunks = [];
    for (var i = 0; i < str.length; i += chunkSize) {
      chunks.add(str.substring(i, math.min(i + chunkSize, str.length)));
    }
    return chunks;
  }

  List<String> chunks = splitIntoChunks(hexString, chunkSize * 2);

  // Widget to dynamically layout the chunks
  return LayoutBuilder(
    builder: (BuildContext context, BoxConstraints constraints) {
      int maxChunksPerRow = (constraints.maxWidth / (chunkSize * 30)).floor();
      maxChunksPerRow =
          math.max(1, maxChunksPerRow); // Ensure at least one chunk per row

      List<Row> rows = [];
      for (var i = 0; i < chunks.length; i += maxChunksPerRow) {
        var rowChunks =
            chunks.sublist(i, math.min(i + maxChunksPerRow, chunks.length));
        rows.add(Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: rowChunks
              .map((chunk) => Text('$chunk ',
                  style: TextStyle(fontFamily: 'Courier', fontSize: 20)))
              .toList(),
        ));
      }

      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: rows,
      );
    },
  );
}
