import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/join_link.dart';

void main() {
  test('channel prefix classifies as LinkKind.channel', () {
    expect(classifyJoinLink('frostsnap://channel/deadbeef'), LinkKind.channel);
  });

  test('keygen prefix classifies as LinkKind.keygen', () {
    expect(classifyJoinLink('frostsnap://keygen/deadbeef'), LinkKind.keygen);
  });

  test('recovery prefix classifies as LinkKind.recovery', () {
    expect(
      classifyJoinLink('frostsnap://recovery/deadbeef'),
      LinkKind.recovery,
    );
  });

  test('empty string is unknown', () {
    expect(classifyJoinLink(''), LinkKind.unknown);
  });

  test('bare scheme with no host is unknown', () {
    expect(classifyJoinLink('frostsnap://'), LinkKind.unknown);
  });

  test('unrelated frostsnap host is unknown', () {
    expect(
      classifyJoinLink('frostsnap://signing/abc'),
      LinkKind.unknown,
      reason:
          'Only the three enumerated hosts are supported. A new '
          'session type must be added here explicitly.',
    );
  });

  test('wrong scheme is unknown', () {
    expect(
      classifyJoinLink('https://frostsnap.com/recovery/x'),
      LinkKind.unknown,
    );
  });

  test('surrounding whitespace is tolerated', () {
    // Paste from clipboard can include trailing newlines; the classifier
    // must not require callers to trim before dispatch.
    expect(
      classifyJoinLink('  frostsnap://recovery/deadbeef\n'),
      LinkKind.recovery,
    );
  });
}
