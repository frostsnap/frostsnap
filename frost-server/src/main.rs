#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::State;
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{error::Error, sync::Mutex};

use schnorr_fun::frost::Nonce;
use schnorr_fun::{frost::PointPoly, fun::Scalar, Signature};

#[derive(Debug)]
pub struct FrostDatabase {
    threshold: usize,
    n_parties: usize,
    polys: BTreeMap<usize, PointPoly>,
    shares: BTreeMap<usize, Vec<Scalar>>,
    pops: BTreeMap<usize, Signature>,
    nonces: BTreeMap<usize, Nonce>,
    sigs: BTreeMap<usize, Scalar>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Response<'a, T> {
    data: T,
    message: &'a str,
}

// Keygen
// Submit point polys
#[post("/keygen", data = "<poly_str>")]
pub fn join_keygen(frost_db: State<Mutex<FrostDatabase>>, poly_str: String) -> Json<usize> {
    let (threshold, n_parties, poly): (usize, usize, PointPoly) =
        serde_json::from_str(&poly_str).unwrap();
    let mut lock = frost_db.lock().unwrap();
    dbg!(&lock.polys);

    let n = lock.polys.len();
    if lock.threshold == 0 && lock.n_parties == 0 {
        lock.threshold = threshold;
        lock.n_parties = n_parties;
    }

    if n >= lock.n_parties {
        dbg!("group already full!");
    } else {
    }
    lock.polys.insert(n, poly);
    Json(n)
}
#[get("/receive_polys")]
pub fn receive_polys(frost_db: State<Mutex<FrostDatabase>>) -> Json<BTreeMap<usize, PointPoly>> {
    let lock = frost_db.lock().unwrap();
    let polys = lock.polys.clone();
    dbg!(&polys);

    if polys.len() < lock.n_parties {
        dbg!("group not full!");
    }

    Json(polys)
}

#[post("/send_shares", data = "<shares_pops>")]
pub fn send_shares(frost_db: State<Mutex<FrostDatabase>>, shares_pops: String) -> Json<String> {
    let (id, shares, pop): (usize, Vec<Scalar>, Signature) =
        serde_json::from_str(&shares_pops).expect("invalid shares");
    let mut lock = frost_db.lock().unwrap();
    lock.shares.insert(id, shares);

    // TODO validate pop
    lock.pops.insert(id, pop);
    Json("true".to_string())
}

#[get("/receive_shares?<i>")]
pub fn receive_shares(
    frost_db: State<Mutex<FrostDatabase>>,
    i: usize,
) -> Json<(Vec<Scalar>, Vec<Signature>)> {
    let lock = frost_db.lock().unwrap();
    let shares = lock.shares.clone();

    let my_shares = shares
        .iter()
        .map(|(_, share_set)| share_set[i].clone())
        .collect();
    dbg!(&my_shares);

    Json((
        my_shares,
        lock.pops.iter().map(|(_, pop)| pop.clone()).collect(),
    ))
}

// SIGNING
// Generate Nonces
#[post("/send_nonce", data = "<nonce>")]
pub fn send_nonce(frost_db: State<Mutex<FrostDatabase>>, nonce: String) -> Json<String> {
    let (index, nonce): (usize, Nonce) = serde_json::from_str(&nonce).expect("invalid shares");
    let mut lock = frost_db.lock().unwrap();
    lock.nonces.insert(index, nonce);
    Json("true".to_string())
}
#[get("/receive_nonces")]
pub fn receive_nonces(frost_db: State<Mutex<FrostDatabase>>) -> Json<Vec<(usize, Nonce)>> {
    let lock = frost_db.lock().unwrap();
    Json(lock.nonces.clone().into_iter().collect())
}

// Sign and share signatures
#[post("/send_sig", data = "<sig>")]
pub fn send_sig(frost_db: State<Mutex<FrostDatabase>>, sig: String) -> Json<String> {
    let (index, sig): (usize, Scalar) = serde_json::from_str(&sig).expect("invalid shares");
    let mut lock = frost_db.lock().unwrap();
    lock.sigs.insert(index, sig);
    Json("true".to_string())
}
#[get("/receive_sigs")]
pub fn receive_sigs(frost_db: State<Mutex<FrostDatabase>>) -> Json<Vec<(usize, Scalar)>> {
    let lock = frost_db.lock().unwrap();
    let sigs = lock.sigs.clone().into_iter().collect();
    Json(sigs)
}

// Clear
#[get("/clear")]
pub fn clear(frost_db: State<Mutex<FrostDatabase>>) {
    // Clear for next time:
    let mut lock = frost_db.lock().unwrap();
    lock.threshold = 0;
    lock.n_parties = 0;
    lock.polys = BTreeMap::new();
    lock.shares = BTreeMap::new();
    lock.pops = BTreeMap::new();
    lock.nonces = BTreeMap::new();
    lock.sigs = BTreeMap::new();
}

fn main() -> Result<(), Box<dyn Error>> {
    rocket::ignite()
        .manage(Mutex::new(FrostDatabase {
            threshold: 0,
            n_parties: 0,
            polys: BTreeMap::new(),
            shares: BTreeMap::new(),
            pops: BTreeMap::new(),
            nonces: BTreeMap::new(),
            sigs: BTreeMap::new(),
        }))
        .mount(
            "/",
            routes![
                join_keygen,
                receive_polys,
                send_shares,
                receive_shares,
                send_nonce,
                receive_nonces,
                send_sig,
                receive_sigs,
                clear,
            ],
        )
        .launch();
    Ok(())
}
