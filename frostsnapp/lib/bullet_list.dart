import 'package:flutter/material.dart';

class BulletList extends StatelessWidget {
  final List<Widget> bullets;

  const BulletList(this.bullets, {super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: bullets.map((bullet) {
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('\u2022'),
            SizedBox(width: 5),
            Flexible(child: bullet),
          ],
        );
      }).toList(),
    );
  }
}
