use frostsnap_embedded_widgets::bitmap::EncodedImage;

fn main() {
    let data = include_bytes!("../../assets/bitcoin-symbol-16x16.bin");
    
    match EncodedImage::from_bytes(data) {
        Ok(image) => {
            println!("Successfully loaded Bitcoin symbol bitmap!");
            println!("Width: {}", image.width());
            println!("Height: {}", image.height());
            println!("Bytes: {} bytes", image.bytes.len());
        }
        Err(e) => {
            println!("Failed to load bitmap: {:?}", e);
        }
    }
}