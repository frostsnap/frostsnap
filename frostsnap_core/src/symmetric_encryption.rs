use chacha20poly1305::{
    aead::{AeadInPlace, KeyInit},
    ChaCha20Poly1305,
};
use core::marker::PhantomData;

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ciphertext<const N: usize, T> {
    data: [u8; N],
    nonce: [u8; 12],
    ty: PhantomData<T>,
    tag: [u8; 16],
}

impl<const N: usize, T: bincode::Encode + bincode::Decode> Ciphertext<N, T> {
    pub fn decrypt(&self, encryption_key: SymmetricKey) -> Option<T> {
        let cipher = ChaCha20Poly1305::new(&encryption_key.0.into());
        let mut plaintext = self.data;
        cipher
            .decrypt_in_place_detached(
                &self.nonce.into(),
                b"",
                &mut plaintext[..],
                &self.tag.into(),
            )
            .ok()?;
        let (value, _) =
            bincode::decode_from_slice(&plaintext, bincode::config::standard()).ok()?;
        Some(value)
    }

    pub fn encrypt(
        encryption_key: SymmetricKey,
        data: &T,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let mut nonce = [0u8; 12];
        rng.fill_bytes(&mut nonce);
        let cipher = ChaCha20Poly1305::new(&encryption_key.0.into());
        let mut ciphertext = [0u8; N];
        let length =
            bincode::encode_into_slice(data, &mut ciphertext[..], bincode::config::standard())
                .expect("programmer error. Couldn't encode into ciphertext.");

        if length != N {
            panic!("encoded plaintext was the wrong length. Expected {N} got {length}");
        }

        let tag = cipher
            .encrypt_in_place_detached(&nonce.into(), b"", &mut ciphertext[..])
            .expect("I don't understand how there could be an error here");

        Self {
            data: ciphertext,
            nonce,
            tag: tag.into(),
            ty: PhantomData,
        }
    }
}

impl<const N: usize> Ciphertext<N, [u8; N]> {
    pub fn random(encrption_key: SymmetricKey, rng: &mut impl rand_core::RngCore) -> Self {
        let mut bytes = [0u8; N];
        rng.fill_bytes(&mut bytes[..]);
        Self::encrypt(encrption_key, &bytes, rng)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SymmetricKey(pub [u8; 32]);
