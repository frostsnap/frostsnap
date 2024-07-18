use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use tracing::{event, Level};

pub type CameraStreamImage = Vec<u8>;

#[derive(Clone, Debug, Default)]
pub struct DecodingProgress {
    pub decoded_frames: usize,
    pub sequence_count: usize,
}

#[derive(Clone, Debug)]
pub enum QrDecoderStatus {
    Progress(DecodingProgress),
    Decoded(Vec<u8>),
    Failed(String),
}

pub fn read_qr_code_bytes(bytes: &[u8]) -> Result<Vec<String>> {
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img,
        Err(e) => {
            return Err(anyhow!(
                "Failed to read in image: {}, bytes: {:?}",
                e,
                bytes
            ))
        }
    };

    let decoder = bardecoder::default_decoder();
    let decoding_results = decoder.decode(&img);

    let decodings = decoding_results
        .into_iter()
        .filter_map(|result| result.ok())
        .collect();

    Ok(decodings)
}

// TODO: Remove this, it should not be necessary after decoding a UR code.
// Figure out why there are a few extra bytes at the beginning of the data coming from the UR decoding.
// It is probably from that dodgy image_converter.dart
fn trim_until_psbt_magic(bytes: &[u8]) -> Option<Vec<u8>> {
    let psbt_magic_bytes = [0x70, 0x73, 0x62, 0x74];
    for i in 0..bytes.len() {
        if i + psbt_magic_bytes.len() <= bytes.len()
            && bytes[i..i + psbt_magic_bytes.len()] == psbt_magic_bytes
        {
            return Some(bytes.split_at(i).1.to_vec());
        }
    }
    None
}

#[derive(Default)]
pub struct FfiQrReader {
    decoder: Arc<Mutex<ur::Decoder>>,
    decoding_progress: Arc<Mutex<DecodingProgress>>,
}

impl FfiQrReader {
    pub fn new() -> Self {
        Self {
            decoder: Default::default(),
            decoding_progress: Arc::new(Mutex::new(DecodingProgress {
                decoded_frames: 0,
                sequence_count: 0,
            })),
        }
    }

    pub fn ingest_ur_strings(&self, qr_strings: Vec<String>) -> Result<QrDecoderStatus> {
        let mut decoder = self.decoder.lock().unwrap();
        let mut decoding_progress = self.decoding_progress.lock().unwrap();

        if decoder.complete() {
            match decoder.message() {
                Ok(message) => {
                    event!(Level::INFO, "Successfully decoded UR code");
                    let raw_psbt =
                        trim_until_psbt_magic(&message.expect("already checked complete"))
                            .expect("found magic bytes");
                    event!(Level::INFO, "Found PSBT magic bytes",);
                    return Ok(QrDecoderStatus::Decoded(raw_psbt));
                }
                Err(e) => return Err(anyhow!("Decoded UR code has inconsistencies: {}", e)),
            }
        }

        for part in qr_strings {
            if part.len() < 3 || part[0..3].to_lowercase() != "ur:" {
                continue; // TODO: return invalid QR error
            }

            let decoding_part = part.to_lowercase();

            // handle SinglePart URs (static QR code)
            match ur::decode(&decoding_part) {
                Err(e) => {
                    event!(Level::WARN, "Failed to decode UR: {}\n{}", e, decoding_part);
                    continue;
                }
                Ok((kind, decoded)) => {
                    if let ur::ur::Kind::SinglePart = kind {
                        event!(Level::INFO, "Successfully decoded UR code");
                        let raw_psbt = match trim_until_psbt_magic(&decoded) {
                            Some(raw_psbt) => raw_psbt,
                            None => {
                                return Err(anyhow!(
                                    "Failed to find PSBT, is this a correct QR code?"
                                ))
                            }
                        };
                        event!(Level::INFO, "Found PSBT magic bytes");
                        return Ok(QrDecoderStatus::Decoded(raw_psbt));
                    }
                }
            }

            // receive multipart (animated QR code)
            match decoder.receive(&decoding_part) {
                Ok(_) => {
                    *decoding_progress = DecodingProgress {
                        sequence_count: decoder.sequence_count(),
                        decoded_frames: decoding_progress.decoded_frames + 1,
                    };
                    event!(Level::INFO, "Read part of UR: {}", decoding_part)
                }
                Err(e) => event!(Level::WARN, "Failed to decode UR: {}\n{}", e, decoding_part),
            }
            if decoder.complete() {
                match decoder.message() {
                    Ok(message) => {
                        event!(Level::INFO, "Successfully decoded UR code.");
                        let raw_psbt = match trim_until_psbt_magic(
                            &message.expect("already checked complete"),
                        ) {
                            Some(raw_psbt) => raw_psbt,
                            None => {
                                return Err(anyhow!(
                                    "Failed to find PSBT, is this a correct QR code?"
                                ))
                            }
                        };
                        event!(Level::INFO, "Found PSBT magic bytes");
                        return Ok(QrDecoderStatus::Decoded(raw_psbt));
                    }
                    Err(e) => return Err(anyhow!("Decoded UR code has inconsistencies: {}", e)),
                }
            }
        }

        event!(Level::INFO, "Scanning progress {:?}", decoding_progress);
        Ok(QrDecoderStatus::Progress(decoding_progress.clone()))
    }
}

pub struct FfiQrEncoder(pub Arc<Mutex<ur::Encoder<'static>>>);

impl FfiQrEncoder {
    pub fn next(&self) -> String {
        let mut encoder = self.0.lock().unwrap();
        encoder.next_part().unwrap()
    }
}
