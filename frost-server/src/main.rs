#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::{Data, State};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use sled_extensions::bincode::Tree;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};

use schnorr_fun::{
    frost::{PointPoly, ScalarPoly},
    fun::{Point, Scalar},
};
use sled_extensions::DbExt;

#[derive(Debug)]
pub struct FrostDatabase {
    polys: BTreeSet<String>,
    shares: BTreeSet<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Response<'a> {
    message: &'a str,
}

#[get("/keygen?<sp>")]
pub fn join_keygen(frost_db: State<Mutex<FrostDatabase>>, sp: String) -> Json<Response> {
    dbg!(&sp);
    let poly: PointPoly = serde_json::from_str(&sp).unwrap();
    dbg!(poly);
    let mut lock = frost_db.lock().unwrap();
    lock.polys.insert(sp);
    dbg!(&lock.shares);
    Json(Response { message: "true" })
}

fn main() -> Result<(), Box<dyn Error>> {
    rocket::ignite()
        .manage(Mutex::new(FrostDatabase {
            polys: BTreeSet::new(),
            shares: BTreeSet::new(),
        }))
        .mount("/", routes![join_keygen])
        .launch();
    Ok(())
}
