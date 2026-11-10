use frostsnap_comms::{firmware_reader, firmware_version::VersionNumber};
use frostsnap_core::hex;
use sha2::{Digest, Sha256};

pub struct ImageInfo {
    pub body_parsed: bool,
    pub version: Option<VersionNumber>,
}

/// `firmware_size` only walks segment headers, so a truncated image can claim to end past its
/// last byte.
fn body_sizes(firmware: &frostsnap_coordinator::FirmwareBin, len: usize) -> Option<(u32, u32)> {
    let (firmware_size, total_size) = firmware_reader::firmware_size(firmware).ok()?;
    (firmware_size <= total_size && total_size as usize <= len)
        .then_some((firmware_size, total_size))
}

pub fn print(image: Vec<u8>) -> ImageInfo {
    let bytes: &'static [u8] = Box::leak(image.into_boxed_slice());
    let firmware = frostsnap_coordinator::FirmwareBin::new(bytes);

    let body = body_sizes(&firmware, bytes.len());
    match body {
        Some((firmware_size, total_size)) => {
            println!(
                "  Size: {} bytes ({} firmware + {} signature block)",
                total_size,
                firmware_size,
                total_size - firmware_size
            );
            let digest = Sha256::digest(&bytes[..firmware_size as usize]);
            println!("  Firmware digest:   {}", hex::encode(&digest));
        }
        None => println!("  Firmware body:     unparseable"),
    }
    let version = firmware_reader::embedded_version(&firmware);
    match &version {
        Ok(Some(version)) => println!("  Version: v{version}"),
        Ok(None) => println!("  Version: none embedded"),
        Err(e) => println!("  Version: unreadable ({e})"),
    }
    ImageInfo {
        body_parsed: body.is_some(),
        version: version.ok().flatten(),
    }
}
