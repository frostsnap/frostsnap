use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use std::str::FromStr;
use tracing::{event, Level};

pub struct QrReader {
    decoder: ur::Decoder,
    decoding_progress: DecodingProgress,
}

impl QrReader {
    #[frb(sync)]
    pub fn new() -> Self {
        Self {
            decoder: Default::default(),
            decoding_progress: DecodingProgress {
                decoded_frames: 0,
                sequence_count: 0,
            },
        }
    }

    pub fn decode_from_bytes(&mut self, bytes: Vec<u8>) -> Result<QrDecoderStatus> {
        let decoded_qr = read_qr_code_bytes(&bytes)?;
        let decoded_ur = self.ingest_ur_strings(decoded_qr)?;
        Ok(decoded_ur)
    }

    pub fn find_address_from_bytes(&self, bytes: Vec<u8>) -> Result<Option<String>> {
        let decoded_qr = read_qr_code_bytes(&bytes)?;
        for maybe_addr in decoded_qr {
            match bitcoin::Address::from_str(&maybe_addr) {
                Ok(_) => return Ok(Some(maybe_addr)),
                Err(_) => continue,
            }
        }
        Ok(None)
    }

    pub fn ingest_ur_strings(&mut self, qr_strings: Vec<String>) -> Result<QrDecoderStatus> {
        let decoder = &mut self.decoder;
        let decoding_progress = &mut self.decoding_progress;

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
                        sequence_count: decoder.sequence_count() as u32,
                        decoded_frames: decoding_progress.decoded_frames + 1_u32,
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

pub struct QrEncoder(ur::Encoder<'static>);

impl QrEncoder {
    #[frb(sync)]
    pub fn new(bytes: Vec<u8>) -> Self {
        let mut length_bytes = bytes.len().to_be_bytes().to_vec();
        while length_bytes.len() > 1 && length_bytes[0] == 0 {
            length_bytes.remove(0);
        }

        // prepending OP_PUSHDATA1 and length for CBOR
        let mut encode_bytes = Vec::new();
        encode_bytes.extend_from_slice(&[0x59]);
        encode_bytes.extend_from_slice(&length_bytes);
        encode_bytes.extend_from_slice(&bytes);

        QrEncoder(ur::Encoder::new(&encode_bytes, 400, "crypto-psbt").unwrap())
    }
    pub fn next_part(&mut self) -> String {
        self.0.next_part().unwrap().to_uppercase()
    }
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

#[derive(Clone, Debug, Default)]
pub struct DecodingProgress {
    pub decoded_frames: u32,
    pub sequence_count: u32,
}

#[derive(Clone, Debug)]
pub enum QrDecoderStatus {
    Progress(DecodingProgress),
    Decoded(Vec<u8>),
    Failed(String),
}
