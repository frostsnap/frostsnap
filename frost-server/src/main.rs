#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::State;
use rocket_contrib::json::Json;
use schnorr_fun::fun::marker::NonZero;
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
    let poly: PointPoly = serde_json::from_str(&poly_str).unwrap();
    let mut lock = frost_db.lock().unwrap();
    let id = lock.polys.len();
    lock.polys.insert(id, poly);
    Json(id)
}
#[get("/receive_polys")]
pub fn receive_polys(frost_db: State<Mutex<FrostDatabase>>) -> Json<BTreeMap<usize, PointPoly>> {
    let lock = frost_db.lock().unwrap();
    let polys = lock.polys.clone();
    dbg!(&polys);
    // TODO error checking
    // if polys.len() <= 1 {
    //     return Json();
    // }
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

fn main() -> Result<(), Box<dyn Error>> {
    rocket::ignite()
        .manage(Mutex::new(FrostDatabase {
            threshold: 2,
            n_parties: 2,
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
                receive_sigs
            ],
        )
        .launch();
    Ok(())
}
