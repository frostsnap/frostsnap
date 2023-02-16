use anyhow::{bail, Result};
use log::*;

use embedded_svc::{
    http::client::*,
    io::Write,
};
use esp_idf_svc::http::client::*;

pub fn request(method: Method, url: impl AsRef<str>, data: Option<&String>) -> Result<String> {
    info!("About to {:?} content from {}", method, url.as_ref());

    let mut client = Client::wrap(EspHttpConnection::new(&Configuration {
        crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
        ..Default::default()
    })?);

    let default_body = String::from("");
    let body = data.unwrap_or(&default_body);
    let bodylen = format!("{}", body.as_bytes().len());
    // info!("{}", body);
    let header = [
        ("Content-Type", "application/json"),
        ("Content-Length", bodylen.as_str()),
    ];
    // info!("Header: {:?}", header);

    let request = client.connection();
    request.initiate_request(method, url.as_ref(), &header)?;
    request.write_all(body.as_bytes())?;
    request.initiate_response()?;
    info!("Response initiated");

    let status = request.status();
    let mut res = String::from("");
    let mut _total_size = 0;
    match status {
        200..=299 => loop {
            let mut buf = [0_u8; 256];
            let read = request.read(&mut buf)?;
            if read == 0 {
                break;
            }
            _total_size += read;
            if _total_size > 20000 {
                bail!("response exceeds 20kB");
            }
            res += &std::str::from_utf8(&buf[..read])?.to_string();
        },
        _ => bail!("unexpected response code: {}", status),
    }
    info!("status: {}, length: {}", status, _total_size);
    Ok(res)
}
