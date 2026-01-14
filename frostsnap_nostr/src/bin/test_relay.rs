use nostr_relay_builder::prelude::*;

pub const TEST_RELAY_PORT: u16 = 7447;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let builder = RelayBuilder::default().port(TEST_RELAY_PORT);
    let relay = LocalRelay::run(builder).await?;
    println!("Test relay running at: {}", relay.url());

    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");
    Ok(())
}
