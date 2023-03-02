use serde_json::json;
use std::error::Error;
use std::io::{self, Write};
use std::str;
use std::thread::sleep;
use std::time::Duration;

fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("can not read user input");
    let cleaned_input = input.trim().to_string();
    cleaned_input
}

fn fetch_input(prompt: &str) -> String {
    println!("{}", prompt);
    read_string()
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Finding serial devices...");
    println!("");
    let found_port: String = loop {
        let ports = serialport::available_ports()?;
        for (port_index, port) in ports.iter().enumerate() {
            println!("{:?} -- {:?}", port_index, port);
        }

        match fetch_input("Type index or enter to refresh: ").parse::<usize>() {
            Ok(index_selection) => break ports[index_selection].port_name.clone(),
            Err(_) => {}
        }
    };

    println!("Connecting to {}", found_port);
    let mut port = serialport::new(&found_port, 115_200)
        .timeout(Duration::from_millis(10))
        .open()
        .unwrap_or_else(|e| {
            eprintln!("Failed to open \"{}\". Error: {}", &found_port, e);
            std::process::exit(1);
        });
    loop {
        let mut writebuf: Vec<u8> = vec![0; 1024];
        write!(
            writebuf.as_mut_slice(),
            "{}",
            json!({ "success": true }).to_string()
        )?;
        writebuf.as_mut_slice().write("fgsfds".as_bytes())?;
        match port.write(&writebuf.as_slice()) {
            Ok(_t) => {
                println!("to client: {}", str::from_utf8(&writebuf[..])?);
                // break;
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => eprintln!("{:?}", e),
        }

        sleep(Duration::from_millis(1000));

        // let mut readbuf: Vec<u8> = vec![0; 1024];
        // match port.read(readbuf.as_mut_slice()) {
        //     Ok(t) => {
        //         let req = str::from_utf8(&readbuf[..t - 1])?;
        //         println!("from client: {}", req);
        //         // let res = request_handler(req, &mut frost_db);

        //         // let mut writebuf: Vec<u8> = vec![0; 1024];
        //         // // write!(writebuf.as_mut_slice(), "{}", json!({ "success": true }).to_string())?;
        //         // writebuf.as_mut_slice().write("fgsfds".as_bytes())?;
        //         // match port.write(&writebuf.as_slice()) {
        //         //     Ok(_t) => {
        //         //         println!("to client: {}", str::from_utf8(&writebuf[..])?);
        //         //         // break;
        //         //     }
        //         //     Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
        //         //     Err(e) => eprintln!("{:?}", e),
        //         // }
        //     }
        //     Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
        //     Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => {
        //         eprintln!("{} disconnected", &found_port);
        //         std::process::exit(1);
        //     }
        //     Err(e) => eprintln!("{:?}", e),
        // }
    }

    // Ok(())
}
