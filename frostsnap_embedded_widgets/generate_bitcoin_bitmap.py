#!/usr/bin/env python3
"""
Generate a Bitcoin symbol bitmap from GNU Unifont.
Creates a binary-encoded bitmap file for use in embedded systems.
"""

from PIL import Image, ImageDraw, ImageFont
import struct
import sys

# Bitcoin symbol Unicode
BITCOIN_SYMBOL = '\u20BF'  # ₿

# Target size for the bitmap (16x16 pixels works well)
SIZE = 16

def generate_bitcoin_bitmap():
    # Load the Unifont
    try:
        font = ImageFont.truetype('assets/unifont-16.0.04.otf', 16)
    except:
        print("Error: Could not load unifont-16.0.04.otf from assets/")
        sys.exit(1)
    
    # Create a new image with white background
    img = Image.new('1', (SIZE, SIZE), color=0)  # '1' = 1-bit pixels (black/white)
    draw = ImageDraw.Draw(img)
    
    # Get the bounding box of the Bitcoin symbol
    bbox = draw.textbbox((0, 0), BITCOIN_SYMBOL, font=font)
    text_width = bbox[2] - bbox[0]
    text_height = bbox[3] - bbox[1]
    
    # Center the symbol in the image
    x = (SIZE - text_width) // 2 - bbox[0]
    y = (SIZE - text_height) // 2 - bbox[1]
    
    # Draw the Bitcoin symbol
    draw.text((x, y), BITCOIN_SYMBOL, font=font, fill=1)
    
    # Convert to bytes for the embedded format
    # Pack pixels into bytes (8 pixels per byte)
    bytes_data = []
    for row in range(SIZE):
        row_bits = 0
        for col in range(8):
            if col < SIZE and img.getpixel((col, row)):
                row_bits |= (1 << (7 - col))
        bytes_data.append(row_bits)
        
        # Second byte for columns 8-15
        if SIZE > 8:
            row_bits = 0
            for col in range(8, min(16, SIZE)):
                if img.getpixel((col, row)):
                    row_bits |= (1 << (15 - col))
            bytes_data.append(row_bits)
    
    # Save as PNG for preview
    img.save('assets/bitcoin-symbol-preview.png')
    print(f"Preview saved to assets/bitcoin-symbol-preview.png")
    
    # Create the EncodedImage structure for bincode
    # Format matches Rust's bincode serialization
    image_color = 0x00  # Binary = 0x00
    width = SIZE
    
    # bincode format for the struct:
    # - ImageColor enum discriminant (1 byte)
    # - width (4 bytes, little-endian u32)
    # - Vec length as varint (bincode uses variable-length encoding for Vec lengths)
    # - bytes data
    
    output = bytearray()
    output.append(image_color)  # ImageColor::Binary
    output.extend(struct.pack('<I', width))  # width as u32 little-endian
    
    # For bincode, Vec length is encoded as a varint
    # For small lengths like 32, it's just the byte value
    # But bincode actually uses a specific format - let's use fixed u64 for compatibility
    vec_len = len(bytes_data)
    # Bincode uses a compact encoding, for length 32 it would be:
    output.append(vec_len)  # For small values < 251, bincode uses single byte
    output.extend(bytes_data)  # actual bytes
    
    # Write the binary file
    with open('assets/bitcoin-symbol-16x16.bin', 'wb') as f:
        f.write(output)
    
    print(f"Bitcoin symbol bitmap generated: {SIZE}x{SIZE} pixels")
    print(f"Binary file saved to assets/bitcoin-symbol-16x16.bin")
    print(f"Total bytes in bitmap: {len(bytes_data)}")
    
    # Print the bytes as Rust array for reference
    print("\n// Bitcoin symbol bitmap data (for reference):")
    print("const BITCOIN_SYMBOL_BYTES: [u8; {}] = [".format(len(bytes_data)))
    for i, byte in enumerate(bytes_data):
        if i % 8 == 0:
            print("    ", end="")
        print(f"0x{byte:02x},", end="")
        if (i + 1) % 8 == 0:
            print()
        else:
            print(" ", end="")
    print("];")

if __name__ == "__main__":
    generate_bitcoin_bitmap()