#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::{Data, State};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use sled_extensions::bincode::Tree;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};

use schnorr_fun::{
    frost::{PointPoly, ScalarPoly},
    fun::{Point, Scalar},
    Signature,
};
use sled_extensions::DbExt;

#[derive(Debug)]
pub struct FrostDatabase {
    threshold: usize,
    nparties: usize,
    polys: Vec<PointPoly>,
    shares: Vec<Vec<Scalar>>,
    pops: Vec<Signature>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Response<'a, T> {
    data: T,
    message: &'a str,
}

// KEYGEN

#[post("/keygen", data = "<poly_str>")]
pub fn join_keygen(frost_db: State<Mutex<FrostDatabase>>, poly_str: String) -> Json<String> {
    let poly: PointPoly = serde_json::from_str(&poly_str).unwrap();
    let mut lock = frost_db.lock().unwrap();
    lock.polys.push(poly);
    Json("true".to_string())
}

#[get("/receive_polys")]
pub fn receive_polys(frost_db: State<Mutex<FrostDatabase>>) -> Json<Vec<PointPoly>> {
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
    let (shares, pop): (Vec<Scalar>, Signature) =
        serde_json::from_str(&shares_pops).expect("invalid shares");
    let mut lock = frost_db.lock().unwrap();
    lock.shares.push(shares);

    // TODO validate pop
    lock.pops.push(pop);
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
        .map(|share_set| share_set[i].clone())
        .collect();
    dbg!(&my_shares);

    // (Vec<Scalar>, Vec<Signature<Public>>)

    Json((my_shares, lock.pops.clone()))
}

// SIGNING

// Main

fn main() -> Result<(), Box<dyn Error>> {
    rocket::ignite()
        .manage(Mutex::new(FrostDatabase {
            threshold: 2,
            nparties: 2,
            polys: vec![],
            shares: vec![],
            pops: vec![],
        }))
        .mount(
            "/",
            routes![join_keygen, receive_polys, send_shares, receive_shares],
        )
        .launch();
    Ok(())
}
