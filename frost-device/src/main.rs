// If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_sys as _;

use schnorr_fun::{
    frost::{Frost, PointPoly, ScalarPoly, XOnlyFrostKey},
    fun::{marker::Public, Scalar},
    musig::NonceKeyPair,
    nonce::Deterministic,
    Message, Schnorr,
};
use sha2::Sha256;

fn main() -> anyhow::Result<()> {
    let threshold: usize = 2;
    let n_parties: usize = 3;

    let frost = Frost::new(Schnorr::<Sha256, Deterministic<Sha256>>::new(
        Deterministic::<Sha256>::default(),
    ));
    dbg!(threshold, n_parties);
    assert!(threshold <= n_parties);

    // create some scalar polynomial for each party
    let mut rng = rand::rngs::OsRng;

    println!("generating scalar polys");
    let scalar_polys = (0..n_parties)
        .map(|_| ScalarPoly::random(threshold, &mut rng))
        .collect::<Vec<_>>();
    println!("converting to point polys");
    let point_polys: Vec<PointPoly> = scalar_polys.iter().map(|sp| sp.to_point_poly()).collect();
    let keygen = frost.new_keygen(point_polys).unwrap();

    println!("creating proofs of possetion and shares");
    let (shares, proofs_of_possesion): (Vec<_>, Vec<_>) = scalar_polys
        .into_iter()
        .map(|scalar_poly| frost.create_shares(&keygen, scalar_poly))
        .unzip();

    // collect the recieved shares for each party
    let mut recieved_shares: Vec<Vec<_>> = vec![];
    for party_index in 0..n_parties {
        println!("Collecting shares for {}", party_index);
        recieved_shares.push(vec![]);
        for share_index in 0..n_parties {
            recieved_shares[party_index].push(shares[share_index][party_index].clone());
        }
    }

    // println!("{:?}", recieved_shares);

    // finish keygen for each party
    let (secret_shares, frost_keys): (Vec<Scalar>, Vec<XOnlyFrostKey>) = (0..n_parties)
        .map(|i| {
            println!("Finishing keygen for participant {}", i);
            let res = frost.finish_keygen(
                keygen.clone(),
                i,
                recieved_shares[i].clone(),
                proofs_of_possesion.clone(),
            );
            match res.clone() {
                Err(e) => {
                    println!("{:?}", e)
                }
                Ok(_) => {}
            }

            let (secret_share, frost_key) = res.unwrap();

            println!("got secret share");
            let xonly_frost_key = frost_key.into_xonly_key();
            (secret_share, xonly_frost_key)
        })
        .unzip();
    println!("Finished keygen.");

    println!("Selecting signers...");

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

    // let mut signatures = vec![];
    // for i in 0..signer_indexes.len() {

    // }
    let signatures = (0..signer_indexes.len())
        .map(|i| {
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
            sig
        })
        .collect();
    let combined_sig = frost.combine_signature_shares(
        &frost_keys[signer_indexes[0]],
        &signing_session,
        signatures,
    );

    println!("verifying final signature");
    assert!(frost.schnorr.verify(
        &frost_keys[signer_indexes[0]].public_key(),
        Message::<Public>::plain("test", b"test"),
        &combined_sig
    ));
    println!("Valid signature!");

    drop(nonces);
    drop(recieved_nonces);
    Ok(())
}
