use alloc::vec::Vec;
use esp_hal::peripherals::DS;
use num_bigint::BigUint;

const ETS_DS_MAX_BITS: usize = 3072;

pub fn ds_sign(ds: DS, encrypted_params: Vec<u8>, challenge: Vec<u8>) -> Vec<u8> {
    let iv = &encrypted_params[..16];
    let ciph = &encrypted_params[16..];
    let y_ciph = &ciph[0..384];
    let m_ciph = &ciph[384..768];
    let rb_ciph = &ciph[768..1152];
    let box_ciph = &ciph[1152..1200];
    if ciph.len() != 1200 {
        panic!("incorrect cipher length!");
    }

    // note: it is probably possible to do these padding
    // and endianess operations without BigUint
    let message_int = BigUint::from_bytes_le(&challenge);
    let mask = (BigUint::from(1u32) << ETS_DS_MAX_BITS) - BigUint::from(1u32);
    let masked_message = message_int & mask;
    let challenge_message = masked_message.to_bytes_be();

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

    for (i, v) in challenge_message.chunks(4).enumerate() {
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
    let mut sig = Vec::new();
    if ds.query_check().read().bits() == 0 {
        // read each 32-bit word and convert to bytes
        for i in 0..(ETS_DS_MAX_BITS / 32) {
            let word = ds.z_mem(i).read().bits();

            // convert u32 to bytes and extend our vector
            sig.extend_from_slice(&word.to_be_bytes());
        }
    } else {
        panic!("Failed to read signature from DS!")
    }

    ds.set_finish().write(|w| w.set_finish().set_bit());
    while ds.query_busy().read().query_busy().bit() {}

    sig
}
