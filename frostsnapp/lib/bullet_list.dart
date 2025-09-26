import 'package:flutter/material.dart';

class BulletList extends StatelessWidget {
  final List<Widget> bullets;

  const BulletList(this.bullets, {super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: bullets.asMap().entries.map((entry) {
        final index = entry.key;
        final bullet = entry.value;
        return Padding(
          padding: EdgeInsets.only(
            bottom: index < bullets.length - 1 ? 8.0 : 0,
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('\u2022'),
              SizedBox(width: 8.0),
              Flexible(child: bullet),
            ],
          ),
        );
      }).toList(),
    );
  }
}
