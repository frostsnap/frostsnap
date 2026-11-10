import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

Duration ms(int n) => Duration(milliseconds: n);

DeviceDemoCue cue({
  required int contactInMs,
  required List<(String, int)> clips,
  required int settleInMs,
  int offsetX = 0,
  int offsetY = 0,
}) => DeviceDemoCue(
  contactInMs: contactInMs,
  clips: [
    for (final (asset, startInMs) in clips)
      DeviceDemoClip(asset: asset, startInMs: startInMs),
  ],
  settleInMs: settleInMs,
  offsetX: offsetX,
  offsetY: offsetY,
);

void main() {
  test('there is no hand until a clip starts', () {
    final track = HandTrack();
    expect(track.poseAt(ms(5000)), isNull);
    expect(track.isMovingAt(ms(5000)), isFalse);

    track.cue(
      cue(contactInMs: 1000, clips: [('swipe', 200)], settleInMs: 3000),
      ms(0),
    );
    expect(track.poseAt(ms(100)), isNull);
    expect(track.poseAt(ms(700))?.elapsed, ms(500));
  });

  test(
    'a hold plays its press, keeps its last frame, then plays its release',
    () {
      final track = HandTrack()
        ..cue(
          cue(
            contactInMs: 1500,
            clips: [('press', -17), ('release', 6000)],
            settleInMs: 8000,
          ),
          ms(0),
        );

      final pressing = track.poseAt(ms(1500));
      expect(pressing?.play.asset, 'press');
      expect(pressing?.elapsed, ms(1517));

      // Long past the press clip's end, so the player shows its last frame: the finger down.
      expect(track.poseAt(ms(5999))?.play.asset, 'press');
      expect(track.poseAt(ms(6000))?.play.asset, 'release');
      expect(track.isMovingAt(ms(7999)), isTrue);
      expect(track.isMovingAt(ms(8000)), isFalse);
    },
  );

  test('the hand rests on the last clip until the next gesture starts', () {
    final track = HandTrack()
      ..cue(
        cue(contactInMs: 1000, clips: [('swipe', 0)], settleInMs: 3000),
        ms(0),
      );
    final first = track.poseAt(ms(2000))?.play;

    track.cue(
      cue(contactInMs: 1000, clips: [('swipe', 300)], settleInMs: 3000),
      ms(5000),
    );
    expect(track.poseAt(ms(5100))?.play, first);
    final second = track.poseAt(ms(5300))?.play;
    expect(second?.asset, 'swipe');
    expect(second, isNot(first));
  });

  test('a tap moves the hand onto its target as the finger reaches for it', () {
    final track = HandTrack();
    expect(track.offsetAt(ms(0)), Offset.zero);

    track.cue(
      cue(
        contactInMs: 300,
        clips: [('tap', -100)],
        settleInMs: 1100,
        offsetX: -79,
        offsetY: -68,
      ),
      ms(1000),
    );
    expect(track.offsetAt(ms(1000)), Offset.zero);
    expect(track.offsetAt(ms(1300)), const Offset(-79, -68));
    expect(track.offsetAt(ms(1900)), const Offset(-79, -68));

    track.cue(
      cue(
        contactInMs: 300,
        clips: [('tap', -100)],
        settleInMs: 1100,
        offsetX: 75,
        offsetY: 40,
      ),
      ms(2000),
    );
    expect(track.offsetAt(ms(2150)), const Offset(-2, -14));

    track.cue(
      cue(contactInMs: 1500, clips: [('press', 0)], settleInMs: 7000),
      ms(3000),
    );
    expect(track.offsetAt(ms(4500)), Offset.zero);
  });
}
