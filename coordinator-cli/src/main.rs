use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::str;
use std::thread::sleep;
use std::time::Duration;
use std::{error::Error, sync::Mutex};

use schnorr_fun::frost::Nonce;
use schnorr_fun::{frost::PointPoly, fun::Scalar, Signature};

#[derive(Debug)]
pub struct FrostDatabase {
    threshold: usize,
    n_parties: usize,
    count: i32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut i = 0;
    let mut frost_db = FrostDatabase {
        threshold: 2,
        n_parties: 3,
        count: i,
    };
    let mut found_port = String::new();
    println!("Waiting for esp32");
    loop {
        let ports = serialport::available_ports()?;
        println!("{:?}", ports);
        if ports.len() > 0 {
            found_port.push_str(ports[0].port_name.as_str());
            break;
        } 
        sleep(Duration::from_millis(1000));
    }

    println!("Connecting to {}", found_port);
    let mut port = serialport::new(&found_port, 115_200)
        .timeout(Duration::from_millis(10))
        .open()
        .unwrap_or_else(|e| {
            eprintln!("Failed to open \"{}\". Error: {}", &found_port, e);
            std::process::exit(1);
        });
    loop {
        // let mut writebuf: Vec<u8> = vec![0; 1024];
        // // write!(writebuf.as_mut_slice(), "{}", json!({ "success": true }).to_string())?;
        // writebuf.as_mut_slice().write("fgsfds".as_bytes())?;
        // match port.write(&writebuf.as_slice()) {
        //     Ok(_t) => {
        //         println!("to client: {}", str::from_utf8(&writebuf[..])?);
        //         // break;
        //     }
        //     Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
        //     Err(e) => eprintln!("{:?}", e),
        // }

        // sleep(Duration::from_millis(1000));

        let mut readbuf: Vec<u8> = vec![0; 1024];
        match port.read(readbuf.as_mut_slice()) {
            Ok(t) => {
                let req = str::from_utf8(&readbuf[..t - 1])?;
                println!("from client: {}", req);
                // let res = request_handler(req, &mut frost_db);

                // let mut writebuf: Vec<u8> = vec![0; 1024];
                // // write!(writebuf.as_mut_slice(), "{}", json!({ "success": true }).to_string())?;
                // writebuf.as_mut_slice().write("fgsfds".as_bytes())?;
                // match port.write(&writebuf.as_slice()) {
                //     Ok(_t) => {
                //         println!("to client: {}", str::from_utf8(&writebuf[..])?);
                //         // break;
                //     }
                //     Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                //     Err(e) => eprintln!("{:?}", e),
                // }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => {
                eprintln!("{} disconnected", &found_port);
                std::process::exit(1);
            }
            Err(e) => eprintln!("{:?}", e),
        }
    }

    // Ok(())
}
