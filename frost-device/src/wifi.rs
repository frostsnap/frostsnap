use anyhow::{bail, Result};
// use embedded_svc::ping::{Summary, Reply};
use log::*;
use std::time::Duration;

use embedded_svc::ipv4;
use embedded_svc::ping::Configuration as PingConf;
use embedded_svc::wifi::*;
use std::net::Ipv4Addr;

// use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::*;
use esp_idf_svc::netif::*;
use esp_idf_svc::ping;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
// use esp_idf_hal::prelude::*;

pub fn wifi(wifi_ssid: &str, wifi_psk: &str, modem: Modem) -> Result<EspWifi<'static>> {

    // let modem = Peripherals::take().unwrap().modem;
    let sysloop = EspSystemEventLoop::take()?;

    // load wifi calibration data
    let p = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(p))?;

    info!("Wifi created, about to scan");

    let ap_infos = wifi.scan()?;

    let ours = ap_infos.into_iter().find(|a| a.ssid == wifi_ssid);

    let channel = if let Some(ours) = ours {
        info!(
            "Found configured access point {} on channel {}",
            wifi_ssid, ours.channel
        );
        Some(ours.channel)
    } else {
        info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            wifi_ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: wifi_ssid.into(),
        password: wifi_psk.into(),
        channel,
        ..Default::default()
    }))?;

    wifi.start()?;

    info!("Starting wifi...");

    if !WifiWait::new(&sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }

    info!("Connecting wifi...");

    wifi.connect()?;

    if !EspNetifWait::new::<EspNetif>(wifi.sta_netif(), &sysloop)?.wait_with_timeout(
        Duration::from_secs(20),
        || {
            wifi.is_connected().unwrap()
                && wifi.sta_netif().get_ip_info().unwrap().ip != Ipv4Addr::new(0, 0, 0, 0)
        },
    ) {
        bail!("Wifi did not connect or did not receive a DHCP lease");
    }

    Ok(wifi)
}

pub fn ping(ip: ipv4::Ipv4Addr) -> Result<()> {
    // info!("About to do some pings for {:?}", ip);

    let ping_conf = PingConf {
        count: 1,
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(1),
        data_size: 56,
        tos: 0,
    };

    ping::EspPing::default().ping(ip, &ping_conf)?;

    // let ping_summary = ping::EspPing::default().ping(ip, &ping_conf)?;
    // if ping_summary.transmitted != ping_summary.received {
    //     bail!("Pinging IP {} resulted in timeouts", ip);
    // }

    // let ping_summary = ping::EspPing::default().ping_details(ip, &ping_conf, &reply)?;
    // if ping_summary.transmitted != ping_summary.received {
    //     bail!("Pinging IP {} resulted in timeouts", ip);
    // }

    // info!("Pinging done");

    Ok(())
}
