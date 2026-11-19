// The phone's copy for the split card flows (design §5.2, §5.3). One place,
// so a line reads the same on every page that shows it.

/// The intro, while the device prepares the card.
const introLine =
    'Your backup is a 64-word card. Your device shows words 1–32 and this '
    'phone shows words 33–64. Write each word next to its number.';

/// Under the intro.
const privacyLine =
    "Do this alone: no calls, no screen sharing. Never let a camera see your "
    'card.';

/// The experimental label: the channel to the device isn't built (design §8).
const experimentalLabel =
    "Experimental. It doesn't yet protect your words from this phone: "
    "the link to your device isn't encrypted.";

/// A device whose card's backup was started, is being checked, or says it
/// was checked, on another phone (design §3.6; spec_v3 PK-O7; owner B9: one
/// phone per check). Neutral, since the phone can't tell an honest second
/// phone from a device that lies: nothing here says the card is checked.
const anotherPhoneLine =
    "This device says this card's backup was started, or checked, on "
    'another phone. A card is checked only on the phone that started it. '
    "This phone hasn't checked it.";

/// Owner B15: only the install that created the wallet runs the verified
/// check. Any other install may write the card, with both quizzes, but never
/// verifies it, and says so on every page. DRAFT COPY, for the owner's review.
const unverifiedTitle = "This phone can't verify this backup";
const unverifiedLine =
    "This phone didn't create this wallet, so it can't check your card. You "
    'can still write it here: your device shows words 1–32 and this phone '
    'shows words 33–64, with a quiz on each half.';
const unverifiedVerifyLine =
    'A card written here is not verified. It must be verified on a trusted '
    'device that learns the entire share: all 64 words.';

/// Review B15 protocol F1: says what writing here costs on the phone that
/// created the wallet: it can't check the card, so it keeps asking for the
/// backup and won't show receive addresses (its card lock has no Later).
const unverifiedKeygenPhoneLine =
    'If you still have the phone that created this wallet, write the card '
    "there instead: it checks the card. Once it's written here, that phone "
    "can't check it either: it will keep asking you to finish this backup, "
    "and won't show receive addresses for this wallet.";
const unverifiedWriteButton = 'Write it unverified';
const unverifiedBanner =
    "Not verified: this phone didn't create this wallet, so it can't check "
    'this card.';
const unverifiedWrittenTitle = 'Written, not verified';
const unverifiedWrittenLine =
    'Both halves are written and both quizzes passed, but this backup is not '
    'verified.';

/// The backup checklist's row for an unverified card (owner B15). DRAFT.
const unverifiedRow = 'Written, not verified';

/// The checklist's note on a wallet that isn't a card wallet made on this
/// phone (owner B15). It's true for a 25-word wallet made on this phone before
/// cards too, which looks the same here (review B15 protocol F3). DRAFT.
const unverifiedChecklistLine =
    'This phone checks a card only for a wallet it created with a card '
    'backup. A key made with a 25-word backup, or restored, keeps the backup '
    "it has. A device whose card isn't written yet can show it here, "
    'unverified.';

/// Owner B9: the question after an interruption of the check (after N).
const interruptedTitle = 'Was that you?';
const interruptedDisconnectedLine =
    'Your device disconnected during the backup check.';
const interruptedClosedLine =
    'The backup check was interrupted: you left this page, this app was '
    'closed, or your device disconnected.';
const interruptedSecurityLine =
    'This matters for security: a faulty or malicious device could '
    'disconnect on purpose to get another try at the check.';
const interruptedQuestion =
    'Did you cause it by accident? For example, did you knock the cable, '
    'unplug your device, leave this page or close this app?';

/// Review F1, APP-2: a device that froze, or told the user to unplug it, made
/// the user leave: that's "no", so the device is marked.
const interruptedFrozenLine =
    "If your device stopped responding, told you to unplug it, or did "
    "something you didn't expect, choose No.";

/// Owner R2-2, review B15 protocol F2: on the question whose "yes" is the last
/// resume of this check, said before the user answers. DRAFT COPY.
const interruptedLastResumeLine =
    "If you answer Yes, the check resumes for the last time. If it's "
    "interrupted again, it ends and this device can't be trusted.";

/// Owner R2-2: on the question asked after the last resume was used, said
/// before the user answers. Either answer ends the check with the device
/// suspect. DRAFT COPY.
const interruptedTooOftenLine =
    "This check has been interrupted too many times. Whatever you answer, it "
    "ends now and this device can't be trusted.";
const interruptedYes = 'Yes, it was me';
const interruptedNo = "No, I didn't";
const interruptedNoHold = 'Hold to answer No.';
const interruptedNoLine =
    "If you didn't, this device can't be trusted, and you'll set this wallet "
    'up again without it.';

/// The phone round's lead (design O11): a lookalike may stand in for the
/// word written.
const roundLine =
    "Pick the word you wrote. If it isn't there, pick None of these.";

/// Under every alarm.
const lineH =
    'No one from Frostsnap will ask for your words, what your device shows, '
    'or to see your screen.';
const helpLine = 'Get help only from the links in Settings > About.';

/// The restore's standing line (design §5.3 E1, E2; critique nit 24).
const restoreHalfLine = 'Words 33–64 only. Enter words 1–32 on your device.';
const neverTypeDeviceHalf = 'Never type words 1–32 into this phone.';

/// "words 45–47", "word 64".
String wordRange(int first, int last) =>
    first == last ? 'word $first' : 'words $first–$last';
