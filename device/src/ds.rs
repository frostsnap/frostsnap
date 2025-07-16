use alloc::vec::Vec;
use esp_hal::peripherals::DS;
use frostsnap_comms::factory::pad_message_for_rsa;
use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
use sha2::Digest;

pub fn standard_rsa_sign(ds: DS, encrypted_params: Vec<u8>, message: &[u8]) -> [u32; 96] {
    // Calculate message digest and apply padding
    let message_digest = sha2::Sha256::digest(message);
    let padded_message = pad_message_for_rsa(&message_digest);
    let sig = private_exponentiation(ds, encrypted_params, padded_message);
    sig
}

pub fn ds_words_to_bytes(words: &[u32; 96]) -> [u8; 384] {
    let mut result = [0u8; 384];
    for (i, &word) in words.iter().rev().enumerate() {
        let bytes = word.to_be_bytes();
        let start = i * 4;
        result[start..start + 4].copy_from_slice(&bytes);
    }
    result
}

pub fn private_exponentiation(
    ds: DS,
    encrypted_params: Vec<u8>,
    mut challenge: [u8; 384],
) -> [u32; 96] {
    challenge.reverse();

    let iv = &encrypted_params[..16];
    let ciph = &encrypted_params[16..];
    let y_ciph = &ciph[0..384];
    let m_ciph = &ciph[384..768];
    let rb_ciph = &ciph[768..1152];
    let box_ciph = &ciph[1152..1200];
    if ciph.len() != 1200 {
        panic!("incorrect cipher length!");
    }

    ds.set_start().write(|w| w.set_start().set_bit());
    while ds.query_busy().read().query_busy().bit() {
        // text_display!(display, "Checking DS Key");
    }
    if ds.query_key_wrong().read().query_key_wrong().bits() == 0 {
        // text_display!(display, "DS Ready");
    } else {
        panic!("DS key read error!");
    }

    for (i, v) in iv.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.iv_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in challenge.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.x_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in y_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.y_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in m_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.m_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in rb_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.rb_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in box_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        ds.box_mem(i).write(|w| unsafe { w.bits(data) });
    }

    ds.set_continue().write(|w| w.set_continue().set_bit());
    while ds.query_busy().read().query_busy().bit_is_set() {}

    let mut sig = [0u32; 96];
    if ds.query_check().read().bits() == 0 {
        for i in 0..(DS_KEY_SIZE_BITS / 32) {
            let word = ds.z_mem(i).read().bits();
            sig[i] = word;
        }
    } else {
        panic!("Failed to read signature from DS!")
    }

    ds.set_finish().write(|w| w.set_finish().set_bit());
    while ds.query_busy().read().query_busy().bit() {}

    sig
}
