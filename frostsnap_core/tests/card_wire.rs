//! The split card's wire messages (design §3.1): every variant round-trips through the frame
//! enums with its tag, the frames stay well under the link's 32 KiB, and no word-carrying
//! message shows a word in its `Debug` or its gist (design §3.11 #16).

use frostsnap_core::device::card::VerdictReason;
use frostsnap_core::message::card::*;
use frostsnap_core::message::{
    CoordinatorRestoration, CoordinatorToDeviceMessage, DeviceToCoordinatorMessage,
};
use frostsnap_core::{EnterPhysicalId, Gist};
use schnorr_fun::frost::ShareImage;
use schnorr_fun::fun::prelude::*;

fn cfg() -> bincode::config::Configuration {
    bincode::config::standard()
}

fn encode<T: bincode::Encode>(value: T) -> Vec<u8> {
    bincode::encode_to_vec(value, cfg()).unwrap()
}

/// The device frame as the core encodes it, after a decode and a re-encode gives the same bytes.
fn device_round_trip(card: DeviceCard) -> Vec<u8> {
    let bytes = encode(DeviceToCoordinatorMessage::Card(card));
    let (decoded, read): (DeviceToCoordinatorMessage, _) =
        bincode::decode_from_slice(&bytes, cfg()).unwrap();
    assert_eq!(read, bytes.len());
    assert_eq!(encode(decoded), bytes);
    bytes
}

fn coordinator_round_trip(card: CoordinatorCard) -> Vec<u8> {
    let message = CoordinatorToDeviceMessage::Card(card.clone());
    let bytes = encode(&message);
    let (decoded, read): (CoordinatorToDeviceMessage, _) =
        bincode::decode_from_slice(&bytes, cfg()).unwrap();
    assert_eq!(read, bytes.len());
    match decoded {
        CoordinatorToDeviceMessage::Card(back) => assert_eq!(back, card),
        other => panic!("decoded as {other:?}"),
    }
    bytes
}

fn device_tag(card: DeviceCard) -> u8 {
    device_round_trip(card)[1]
}

fn coordinator_tag(card: CoordinatorCard) -> u8 {
    coordinator_round_trip(card)[1]
}

fn status(reason: Option<VerdictReason>) -> CardStatus {
    CardStatus {
        share_image: ShareImage {
            index: s!(3).public(),
            image: g!(42 * G).normalize().mark_zero(),
        },
        format_id: 3,
        proof_version: 3,
        sealed: CardSealed {
            format_id: 3,
            h_c: [0xC5; 32],
            coin_commit: [0xC0; 32],
        },
        state: CardStateReport::SelfCheck,
        check: CardCheckReport::DeviceRoundPending,
        reason,
    }
}

/// Word index 1234 (0x04D2), a sentinel no counter or tag here takes.
const SENTINEL: u16 = 1234;

#[test]
fn device_frames_round_trip_with_their_tags() {
    assert_eq!(device_tag(DeviceCard::CheckDone), 3);
    assert_eq!(device_tag(DeviceCard::GateShown), 8);
    for pass in [false, true] {
        assert_eq!(device_tag(DeviceCard::DeviceRound { pass }), 9);
    }
    for result in [
        RestoreResult::Restored,
        RestoreResult::CouldNotRead,
        RestoreResult::AlreadyHeld,
        RestoreResult::NeedsNewerFirmware,
        RestoreResult::WrongWallet,
    ] {
        // The result alone: nothing decoded leaves the device before the hold (owner B7).
        assert_eq!(device_tag(DeviceCard::RestoreOutcome { result }), 10);
    }
    for bits in 0..8u8 {
        let state = DeviceCard::RestoreState {
            device_half_in: bits & 1 != 0,
            phone_half_in: bits & 2 != 0,
            decoding: bits & 4 != 0,
        };
        assert_eq!(device_tag(state), 11);
    }
    for d_words in [None, Some(CardWords5([SENTINEL; 5]))] {
        let words = DeviceCard::RecheckWords {
            phone_half: CardWords32([SENTINEL; 32]),
            d_words,
        };
        assert_eq!(device_tag(words), 12);
    }
    assert_eq!(device_tag(DeviceCard::RecheckDone), 13);
    // Format C′ (spec PK.5): words 33–64 in one frame.
    assert_eq!(
        device_tag(DeviceCard::PhoneHalf(CardWords32([SENTINEL; 32]))),
        14
    );
}

#[test]
fn coordinator_frames_round_trip_with_their_tags() {
    assert_eq!(
        coordinator_tag(CoordinatorCard::WriteStart { h_c: [0xC5; 32] }),
        2
    );
    assert_eq!(coordinator_tag(CoordinatorCard::FinalBegin), 9);
    for j in [32, 47, 63] {
        assert_eq!(coordinator_tag(CoordinatorCard::OpenPhoneWord { j }), 10);
    }
    for words_hash in [None, Some([7; 32])] {
        assert_eq!(coordinator_tag(CoordinatorCard::Recheck { words_hash }), 11);
    }
    // Retired by format C′, but still decoded (append-only): the device refuses them.
    assert_eq!(
        coordinator_tag(CoordinatorCard::Prepare { h_c: [0xC5; 32] }),
        12
    );
    // Format C′ (spec PK.5, owner B1).
    assert_eq!(
        coordinator_tag(CoordinatorCard::PullPhoneHalf { h_c: [0xC5; 32] }),
        13
    );
    assert_eq!(
        coordinator_tag(CoordinatorCard::DeviceHalfWritten { h_c: [0xC5; 32] }),
        14
    );
}

#[test]
fn status_round_trips() {
    for reason in [None, Some(VerdictReason::DeviceRoundFailed)] {
        let bytes = device_round_trip(DeviceCard::Status(Box::new(status(reason))));
        let (back, _): (CardStatus, _) = bincode::decode_from_slice(&bytes[2..], cfg()).unwrap();
        assert_eq!(back.reason, reason);
        assert_eq!(back.state, CardStateReport::SelfCheck);
    }
}

/// The enums the coordinator persists keep the numbers the pins give.
#[test]
fn card_variant_numbers_are_pinned() {
    assert_eq!(encode(CardCheckReport::DeviceRoundPending), [3]);
    assert_eq!(encode(CardCheckReport::DeviceRoundPassed), [4]);
    assert_eq!(encode(CardRefusal::DeviceError), [14]);
    assert_eq!(encode(CardRefusal::NotInPhoneHalf), [15]);
    assert_eq!(encode(CardRefusal::NoRecord), [16]);
    assert_eq!(encode(VerdictReason::StoppedBeforeN), [2]);
    assert_eq!(encode(VerdictReason::DeviceRoundFailed), [4]);
    for state in [
        CardStateReport::Sealed,
        CardStateReport::Writing,
        CardStateReport::SelfCheck,
        CardStateReport::CardChecked,
        CardStateReport::Proving,
    ] {
        let (back, _): (CardStateReport, _) =
            bincode::decode_from_slice(&encode(state), cfg()).unwrap();
        assert_eq!(back, state);
    }
    // PROVING (owner B1) comes after the other states.
    assert_eq!(encode(CardStateReport::Checking), [4]);
    assert_eq!(encode(CardStateReport::Failed), [6]);
    assert_eq!(encode(CardStateReport::Proving), [7]);
}

/// An opening frame holds its part 0 (the word and 77 Q's) or 39 proofs: well under the
/// 32 KiB a frame may hold (design §3.1). Words 33–64 are one frame of about 50 bytes.
#[test]
fn opening_frames_fit() {
    use frost_backup::verifiable_card::{layout::CARRIES_PER_BOX, proof::OPENING_SPLIT};
    assert_eq!(CARRIES_PER_BOX, 77);
    assert_eq!(OPENING_SPLIT, 39);
    let carries = device_round_trip(DeviceCard::OpeningPart(CardOpeningPart::Carries {
        j: 31,
        y: 2047,
        carries: vec![[0xFF; 33]; CARRIES_PER_BOX],
    }));
    let proofs = device_round_trip(DeviceCard::OpeningPart(CardOpeningPart::Proofs {
        j: 31,
        first: 0,
        proofs: vec![[0xFF; 96]; OPENING_SPLIT],
    }));
    assert!(carries.len() < 2_600, "{}", carries.len());
    assert!(proofs.len() < 3_800, "{}", proofs.len());
    let phone_half = device_round_trip(DeviceCard::PhoneHalf(CardWords32([2047; 32])));
    assert!(phone_half.len() < 100, "{}", phone_half.len());
    let words = device_round_trip(DeviceCard::RecheckWords {
        phone_half: CardWords32([2047; 32]),
        d_words: Some(CardWords5([2047; 5])),
    });
    assert!(words.len() < 120, "{}", words.len());
}

/// Design §3.11 #16: no word index or word in the `Debug` of a word-carrying message, and every
/// new message's gist is its variant name.
#[test]
fn word_messages_are_redacted() {
    let phone_half = CoordinatorRestoration::CardPhoneHalf {
        enter_physical_id: EnterPhysicalId::new(&mut rand::thread_rng()),
        words: CardHalfEntries([Some(SENTINEL); 32]),
    };
    let recheck_words = DeviceCard::RecheckWords {
        phone_half: CardWords32([SENTINEL; 32]),
        d_words: Some(CardWords5([SENTINEL; 5])),
    };
    let opening = DeviceCard::OpeningPart(CardOpeningPart::Carries {
        j: 20,
        y: SENTINEL,
        carries: vec![],
    });
    let device_phone_half = DeviceCard::PhoneHalf(CardWords32([SENTINEL; 32]));
    let printed = [
        format!("{phone_half:?}"),
        format!("{recheck_words:?}"),
        format!("{opening:?}"),
        format!("{device_phone_half:?}"),
        format!(
            "{:?}",
            DeviceToCoordinatorMessage::Card(device_phone_half.clone())
        ),
        format!("{:?}", CardWords32([SENTINEL; 32])),
        format!("{:?}", CardWords5([SENTINEL; 5])),
        format!("{:?}", CardHalfEntries([Some(SENTINEL); 32])),
        format!(
            "{:?}",
            CoordinatorToDeviceMessage::Restoration(phone_half.clone())
        ),
        format!(
            "{:?}",
            DeviceToCoordinatorMessage::Card(recheck_words.clone())
        ),
    ];
    for text in &printed {
        assert!(!text.contains("1234"), "{text}");
        assert!(!text.contains("4d2") && !text.contains("4D2"), "{text}");
    }
    let gists = [
        CoordinatorToDeviceMessage::Card(CoordinatorCard::OpenPhoneWord { j: 40 }).gist(),
        CoordinatorToDeviceMessage::Card(CoordinatorCard::Recheck {
            words_hash: Some([1; 32]),
        })
        .gist(),
        CoordinatorToDeviceMessage::Restoration(phone_half).gist(),
        DeviceToCoordinatorMessage::Card(recheck_words).gist(),
        DeviceToCoordinatorMessage::Card(DeviceCard::RecheckDone).gist(),
        DeviceToCoordinatorMessage::Card(DeviceCard::RestoreState {
            device_half_in: true,
            phone_half_in: false,
            decoding: false,
        })
        .gist(),
        DeviceToCoordinatorMessage::Card(device_phone_half).gist(),
        CoordinatorToDeviceMessage::Card(CoordinatorCard::PullPhoneHalf { h_c: [0xC5; 32] }).gist(),
    ];
    assert_eq!(
        gists,
        [
            "OpenPhoneWord",
            "Recheck",
            "CardPhoneHalf",
            "RecheckWords",
            "RecheckDone",
            "RestoreState",
            "PhoneHalf",
            "PullPhoneHalf",
        ]
    );
    assert_eq!(
        format!("{:?}", CoordinatorCard::OpenPhoneWord { j: 40 }),
        "OpenPhoneWord"
    );
}
