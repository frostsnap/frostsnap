// Necessary because of this issue: https://github.com/rust-lang/cargo/issues/9641
fn main() -> anyhow::Result<()> {
    if std::env::var("CARGO_CFG_TARGET_VENDOR") == Ok("espressif".to_string()) {
        embuild::build::CfgArgs::output_propagated("ESP_IDF")?;
        embuild::build::LinkArgs::output_propagated("ESP_IDF")
    } else {
        Ok(())
    }
}
