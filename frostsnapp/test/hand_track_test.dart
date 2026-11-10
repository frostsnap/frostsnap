import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

Duration ms(int n) => Duration(milliseconds: n);

DeviceDemoCue swipe({required int contactInMs}) => DeviceDemoCue(
  contactInMs: contactInMs,
  gesture: const DeviceDemoGesture.swipe(),
);

DeviceDemoCue hold({required int contactInMs, required int holdMs}) =>
    DeviceDemoCue(
      contactInMs: contactInMs,
      gesture: DeviceDemoGesture.hold(holdMs: holdMs),
    );

DeviceDemoCue tap({required int contactInMs, required int x, required int y}) =>
    DeviceDemoCue(
      contactInMs: contactInMs,
      gesture: DeviceDemoGesture.tap(x: x, y: y),
    );

/// When the hand comes to rest after a cue at zero, to the frame.
Duration settlesAt(DeviceDemoCue cue) {
  final track = HandTrack()..cue(cue, Duration.zero);
  var t = Duration.zero;
  while (track.isMovingAt(t)) {
    t += ms(1);
  }
  return t;
}

void main() {
  test('a swipe cue puts the finger on the screen when the touch lands', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1400), ms(500));

    expect(track.frameAt(ms(500)), HandFrame.rest);
    expect(
      track.frameAt(ms(1900)),
      const HandFrame(HandSheet.swipe, HandTrack.swipeContactFrame),
    );
  });

  test('there is no hand until a gesture is cued', () {
    expect(HandTrack().frameAt(ms(5000)), isNull);
    expect(HandTrack().isMovingAt(ms(5000)), isFalse);
  });

  test(
    'a hold cue presses when the touch lands and stays down for the hold',
    () {
      final track = HandTrack();
      track.cue(hold(contactInMs: 1500, holdMs: 8000), ms(0));

      expect(
        track.frameAt(ms(1500)),
        const HandFrame(HandSheet.hold, HandTrack.holdContactFrame),
      );
      expect(
        track.frameAt(ms(1500 + 4000)),
        const HandFrame(HandSheet.hold, 90),
      );
      // Frame 137 is where the finger lifts in the clip.
      expect(
        track.frameAt(ms(1500 + 8000)),
        const HandFrame(HandSheet.hold, 137),
      );
    },
  );

  test('the timing the demo scripts by is the clips\' own', () {
    final timing = HandTrack.timing;
    const oneFrame = Duration(microseconds: 1000000 ~/ HandTrack.fps);

    final swipeEnd = ms(timing.swipeContactMs + timing.swipeAfterContactMs);
    expect(
      settlesAt(swipe(contactInMs: timing.swipeContactMs)) - swipeEnd,
      lessThanOrEqualTo(oneFrame),
    );

    final holdEnd = ms(
      timing.holdContactMs + timing.holdPressMs + timing.holdAfterReleaseMs,
    );
    expect(
      (settlesAt(
                hold(
                  contactInMs: timing.holdContactMs,
                  holdMs: timing.holdPressMs,
                ),
              ) -
              holdEnd)
          .abs(),
      lessThanOrEqualTo(oneFrame),
    );

    final tapEnd = ms(
      timing.tapContactMs + timing.tapPressMs + timing.tapAfterReleaseMs,
    );
    expect(
      (settlesAt(tap(contactInMs: timing.tapContactMs, x: 120, y: 215)) -
              tapEnd)
          .abs(),
      lessThanOrEqualTo(oneFrame),
    );

    final track = HandTrack()
      ..cue(tap(contactInMs: timing.tapContactMs, x: 120, y: 215), ms(0));
    // Hold frames up to 22 and from 171 are the rest pose.
    for (final edge in [Duration.zero, tapEnd - oneFrame]) {
      expect(
        switch (track.frameAt(edge)) {
          HandFrame(sheet: HandSheet.hold, :final index) =>
            index <= 22 || index >= 171,
          _ => false,
        },
        isTrue,
        reason: '$edge',
      );
    }
    expect(
      track.frameAt(ms(timing.tapContactMs)),
      const HandFrame(HandSheet.hold, HandTrack.holdContactFrame),
    );
    expect(
      track.frameAt(ms(timing.tapContactMs + timing.tapPressMs) + oneFrame),
      isNot(const HandFrame(HandSheet.hold, HandTrack.holdContactFrame)),
    );
  });

  test('a tap moves the finger onto its target as the hand reaches for it', () {
    final track = HandTrack();
    expect(track.offsetAt(ms(0)), Offset.zero);

    track.cue(tap(contactInMs: 300, x: 41, y: 147), ms(1000));
    expect(track.offsetAt(ms(1000)), Offset.zero);
    expect(
      track.offsetAt(ms(1300)),
      const Offset(41, 147) - HandTrack.fingertip,
    );
    expect(
      track.offsetAt(ms(1900)),
      const Offset(41, 147) - HandTrack.fingertip,
    );

    track.cue(tap(contactInMs: 300, x: 195, y: 255), ms(2000));
    expect(
      track.offsetAt(ms(2150)),
      (const Offset(41, 147) + const Offset(195, 255)) / 2 -
          HandTrack.fingertip,
    );

    track.cue(hold(contactInMs: 1500, holdMs: 3000), ms(3000));
    expect(track.offsetAt(ms(4500)), Offset.zero);
  });

  test('the hand keeps moving with nothing arriving from the device', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1000), ms(0));

    final frames = {
      for (var t = 0; t < 1000; t += 1000 ~/ HandTrack.fps)
        track.frameAt(ms(t)),
    };
    expect(frames.length, greaterThan(20));
    expect(track.isMovingAt(ms(999)), isTrue);
  });

  test('after its clip the hand rests in the pose every clip starts from', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1000), ms(0));

    expect(track.frameAt(ms(60000)), HandFrame.rest);
    expect(track.isMovingAt(ms(60000)), isFalse);
  });
}
