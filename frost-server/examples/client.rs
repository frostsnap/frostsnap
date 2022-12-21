use std::error::Error;

use schnorr_fun::Signature;
use schnorr_fun::{
    frost::{Frost, Nonce, PointPoly, ScalarPoly},
    fun::{marker::Public, Scalar},
    nonce::Deterministic,
    Message, Schnorr,
};
use sha2::Sha256;

use serde_json::{json, Value as JsonObj};

fn main() -> Result<(), Box<dyn Error>> {
    // We're going to carry out 2 parties on 1 device and later separate
    let threshold: usize = 2;
    let n_parties: usize = 2;
    // let frost = Frost::new(Schnorr::<Sha256, Deterministic<Sha256>>::new(
    //     Deterministic::<Sha256>::default(),
    // ));
    dbg!(threshold, n_parties);
    assert!(threshold <= n_parties);

    let mut rng = rand::rngs::OsRng;
    let sp = ScalarPoly::random(threshold, &mut rng);
    dbg!(&sp);
    println!("converting to point polys");
    let pp = sp.to_point_poly();
    dbg!(&pp);

    Ok(())
}
