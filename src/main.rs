use esp_idf_sys as _;
use rand::RngCore; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

use secp256kfun::{marker::Public, Scalar};

use schnorr_fun::{
    frost::{Frost, PointPoly, ScalarPoly, XOnlyFrostKey},
    musig::NonceKeyPair,
    nonce::Deterministic,
    Message, Schnorr,
};
use sha2::Sha256;

fn main() {
    // Temporary. Will disappear once ESP-IDF 4.4 is released, but for now it is necessary to call this function once,
    // or else some patches to the runtime implemented by esp-idf-sys might not link properly.
    esp_idf_sys::link_patches();

    let threshold: usize = 2;
    let n_parties: usize = 3;

    let frost = Frost::new(Schnorr::<Sha256, Deterministic<Sha256>>::new(
        Deterministic::<Sha256>::default(),
    ));
    dbg!(threshold, n_parties);
    assert!(threshold <= n_parties);

    // create some scalar polynomial for each party
    let mut scalar_polys = vec![];
    for i in 1..=n_parties {
        println!("Creating scalar poly {}", i);
        let scalar_poly = (1..=threshold)
            .map(|_| {
                let mut rng: rand::rngs::StdRng = rand::SeedableRng::from_entropy();
                Scalar::from(rng.next_u32())
                    .non_zero()
                    .expect("computationally unreachable")
            })
            .collect();
        scalar_polys.push(ScalarPoly::new(scalar_poly));
    }
    let point_polys: Vec<PointPoly> = scalar_polys.iter().map(|sp| sp.to_point_poly()).collect();

    let keygen = frost.new_keygen(point_polys).unwrap();

    let mut proofs_of_possession = vec![];
    let mut shares_vec = vec![];
    for (i, sp) in scalar_polys.into_iter().enumerate() {
        println!("calculating shares and pop {}", i);
        let (shares, pop) = frost.create_shares(&keygen, sp);
        proofs_of_possession.push(pop);
        shares_vec.push(shares);
    }
    println!("Calculated shares and pops");

    // collect the recieved shares for each party
    let mut recieved_shares: Vec<Vec<_>> = vec![];
    for party_index in 0..n_parties {
        println!("Collecting shares for {}", party_index);
        recieved_shares.push(vec![]);
        for share_index in 0..n_parties {
            recieved_shares[party_index].push(shares_vec[share_index][party_index].clone());
        }
    }

    println!("{:?}", recieved_shares);

    // finish keygen for each party
    let (secret_shares, frost_keys): (Vec<Scalar>, Vec<XOnlyFrostKey>) = (0..n_parties)
        .map(|i| {
            println!("Finishing keygen for participant {}", i);

            let res = frost.finish_keygen(
                keygen.clone(),
                i,
                recieved_shares[i].clone(),
                proofs_of_possession.clone(),
            );
            match res.clone() {
                Err(e) => {
                    println!("{:?}", e)
                }
                Ok(_) => {
                    println!("OK!")
                }
            }

            let (secret_share, frost_key) = res.unwrap();

            println!("got secret share");
            let xonly_frost_key = frost_key.into_xonly_key();
            (secret_share, xonly_frost_key)
        })
        .unzip();
    println!("Finished keygen!");

    println!("selecting signers...");

    // use a boolean mask for which t participants are signers
    let mut signer_mask = vec![true; threshold];
    signer_mask.append(&mut vec![false; n_parties - threshold]);
    // shuffle the mask for random signers

    let signer_indexes: Vec<_> = signer_mask
        .iter()
        .enumerate()
        .filter(|(_, is_signer)| **is_signer)
        .map(|(i, _)| i)
        .collect();

    println!("Preparing for signing session...");

    let verification_shares_bytes: Vec<_> = frost_keys[signer_indexes[0]]
        .verification_shares()
        .map(|share| share.to_bytes())
        .collect();

    let sid = [
        frost_keys[signer_indexes[0]]
            .public_key()
            .to_xonly_bytes()
            .as_slice(),
        verification_shares_bytes.concat().as_slice(),
        b"frost-prop-test".as_slice(),
    ]
    .concat();
    let nonces: Vec<NonceKeyPair> = signer_indexes
        .iter()
        .map(|i| {
            frost.gen_nonce(
                &secret_shares[*i],
                &[sid.as_slice(), [*i as u8].as_slice()].concat(),
                Some(frost_keys[signer_indexes[0]].public_key().normalize()),
                None,
            )
        })
        .collect();

    let mut recieved_nonces: Vec<_> = vec![];
    for (i, nonce) in signer_indexes.iter().zip(nonces.clone()) {
        recieved_nonces.push((*i, nonce.public()));
    }
    println!("Recieved nonces..");

    // Create Frost signing session
    let signing_session = frost.start_sign_session(
        &frost_keys[signer_indexes[0]],
        recieved_nonces.clone(),
        Message::plain("test", b"test"),
    );

    let mut signatures = vec![];
    for i in 0..signer_indexes.len() {
        println!("Signing for participant {}", signer_indexes[i]);
        let signer_index = signer_indexes[i];
        let session = frost.start_sign_session(
            &frost_keys[signer_index],
            recieved_nonces.clone(),
            Message::plain("test", b"test"),
        );
        let sig = frost.sign(
            &frost_keys[signer_index],
            &session,
            signer_index,
            &secret_shares[signer_index],
            nonces[i].clone(),
        );
        assert!(frost.verify_signature_share(
            &frost_keys[signer_index],
            &session,
            signer_index,
            sig
        ));
        signatures.push(sig);
    }
    let combined_sig = frost.combine_signature_shares(
        &frost_keys[signer_indexes[0]],
        &signing_session,
        signatures,
    );

    assert!(frost.schnorr.verify(
        &frost_keys[signer_indexes[0]].public_key(),
        Message::<Public>::plain("test", b"test"),
        &combined_sig
    ));
}
