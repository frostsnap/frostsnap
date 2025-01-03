import 'dart:typed_data';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:frostsnapp/theme.dart';

String toHex(Uint8List data) {
  return data.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join('');
}

String toSpacedHex(Uint8List data, {int chunkSize = 2}) {
  return splitIntoChunks(
          data.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join(''),
          chunkSize * 2)
      .join(" ");
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

// Function to split the hex string into chunks
List<String> splitIntoChunks(String str, int chunkSize) {
  List<String> chunks = [];
  for (var i = 0; i < str.length; i += chunkSize) {
    chunks.add(str.substring(i, math.min(i + chunkSize, str.length)));
  }
  return chunks;
}

Widget chunkedAddressFormat(
  String string, {
  int chunkSize = 4,
  Color? textColor,
  Color? backgroundColor,
}) {
  List<String> chunks = splitIntoChunks(string, chunkSize);

  return Align(
    alignment: Alignment.center,
    child: Container(
      padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: backgroundColor != null
          ? BoxDecoration(
              color: backgroundColor,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: Colors.white.withValues(alpha: 26)),
            )
          : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          for (int i = 0;
              i < chunks.length;
              i += 3) // Group chunks in rows of 3
            Row(
              mainAxisSize:
                  MainAxisSize.min, // Wraps the row tightly around the content
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                for (int j = i; j < i + 3 && j < chunks.length; j++)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    child: Text(
                      chunks[j],
                      style: addressTextStyle.copyWith(
                          fontSize: 20, color: textColor),
                    ),
                  ),
              ],
            ),
        ],
      ),
    ),
  );
}
