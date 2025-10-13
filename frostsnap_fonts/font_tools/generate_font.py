#!/usr/bin/env python3
"""
Generate Gray4 anti-aliased fonts with minimal line height for embedded systems.

This script converts TrueType/OpenType fonts to a compact Gray4 format
with 4-bit (16-level) anti-aliasing, with line height set to actual character height.

Usage:
    python generate_font.py <font_path> <size> <output_name> [--weight WEIGHT]

Examples:
    python generate_font.py fonts/NotoSans-Variable.ttf 24 noto_sans_24_bold --weight 700
    python generate_font.py fonts/NotoSans-Variable.ttf 18 noto_sans_18_light --weight 300
    python generate_font.py fonts/NotoSansMono-Variable.ttf 18 noto_sans_mono_18 --weight 400

Weight values (for Variable fonts):
    100 - Thin
    300 - Light
    400 - Regular (default)
    500 - Medium
    700 - Bold
    900 - Black
"""

import sys
import freetype
from pathlib import Path
import argparse

def generate_gray4_font(font_path, font_size, output_name, weight=None, chars_to_include=None):
    """
    Generate a Gray4 font with 16-level anti-aliasing and minimal line height.

    Args:
        font_path: Path to the TrueType/OpenType font file
        font_size: Font size in pixels
        output_name: Name for the output constants (e.g., "NOTO_SANS_24_REGULAR")
        weight: Font weight for Variable fonts (100-900, default: 400)
        chars_to_include: List of characters to include (default: ASCII printable)

    Returns:
        Tuple of (rust_code, total_size_bytes)
    """

    # Default to printable ASCII if no chars specified
    if chars_to_include is None:
        chars_to_include = [chr(i) for i in range(32, 127)]

    # Load font
    face = freetype.Face(str(font_path))

    # Set weight for Variable fonts
    if weight is not None and face.has_multiple_masters:
        try:
            var_info = face.get_variation_info()
            # Find the weight axis (tag 'wght')
            weight_axis = None
            for axis in var_info.axes:
                if axis.tag == 'wght':
                    weight_axis = axis
                    break

            if weight_axis:
                # Clamp weight to valid range
                weight = max(weight_axis.minimum, min(weight, weight_axis.maximum))
                face.set_var_design_coords([weight])
                print(f"Set variable font weight to {weight}")
            else:
                print(f"Warning: No weight axis found in {font_path}")
        except Exception as e:
            print(f"Warning: Could not set weight: {e}")

    face.set_pixel_sizes(0, font_size)
    
    # Collect glyph data
    glyphs = []
    packed_data = bytearray()
    
    # Calculate baseline from ascender
    ascender = face.size.ascender >> 6
    descender = face.size.descender >> 6
    baseline = ascender  # Baseline is at the ascender height
    
    # Track actual max glyph height to calculate minimal line height
    max_glyph_top = 0
    max_glyph_bottom = 0
    
    print(f"Font metrics: ascender={ascender}, descender={descender}, baseline={baseline}")
    print(f"Original line height: {face.size.height >> 6}")
    
    # First pass: collect glyphs and find actual bounds
    for char in chars_to_include:
        face.load_char(char, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_NORMAL)
        glyph = face.glyph
        bitmap = glyph.bitmap
        
        if bitmap.width == 0 or bitmap.rows == 0:
            # Space or other empty character
            glyphs.append({
                'char': char,
                'width': 0,
                'height': 0,
                'x_offset': 0,
                'y_offset': 0,
                'x_advance': glyph.advance.x >> 6,
                'data_offset': len(packed_data)
            })
            continue
        
        # Track actual glyph bounds
        glyph_top = glyph.bitmap_top
        glyph_bottom = glyph.bitmap_top - bitmap.rows
        
        if glyph_top > max_glyph_top:
            max_glyph_top = glyph_top
        if glyph_bottom < max_glyph_bottom:
            max_glyph_bottom = glyph_bottom
        
        # Convert buffer to list and apply gamma correction
        gamma = 1.8
        gray4_values = []

        if len(bitmap.buffer) == 0:
            # Empty buffer - all zeros
            gray4_values = [0] * (bitmap.rows * bitmap.width)
        else:
            # Apply gamma correction for better anti-aliasing
            for byte in bitmap.buffer:
                # Normalize to 0.0-1.0
                normalized = byte / 255.0
                # Apply gamma correction
                corrected = normalized ** (1 / gamma)
                # Convert to 4-bit (0-15)
                gray4_value = int(corrected * 15)
                gray4_values.append(gray4_value)
        
        # Pack into bytes (2 pixels per byte)
        glyph_data = []
        pixels = gray4_values
        for i in range(0, len(pixels), 2):
            if i + 1 < len(pixels):
                # Pack two pixels: high nibble = first pixel, low nibble = second
                byte = (pixels[i] << 4) | pixels[i + 1]
            else:
                # Last pixel if odd number
                byte = pixels[i] << 4
            glyph_data.append(byte)
        
        # Store glyph info with bitmap_top for later y_offset calculation
        glyphs.append({
            'char': char,
            'width': bitmap.width,
            'height': bitmap.rows,
            'x_offset': glyph.bitmap_left,
            'bitmap_top': glyph.bitmap_top,  # Store for later calculation
            'x_advance': glyph.advance.x >> 6,
            'data_offset': len(packed_data)
        })
        
        packed_data.extend(glyph_data)
    
    # Sort glyphs by character code for binary search
    glyphs.sort(key=lambda g: ord(g['char']))
    
    # Calculate minimal line height based on actual glyph bounds
    minimal_line_height = max_glyph_top - max_glyph_bottom
    
    # Adjust baseline to be relative to the minimal line height
    # The baseline should be the distance from the top of the minimal line to the baseline
    adjusted_baseline = max_glyph_top
    
    # Now calculate y_offsets for each glyph using the adjusted baseline
    for glyph in glyphs:
        if 'bitmap_top' in glyph:
            # Calculate y_offset: distance from top of line to top of glyph
            # adjusted_baseline is the top of the highest glyph
            # bitmap_top is where this glyph's top is relative to the baseline
            glyph['y_offset'] = adjusted_baseline - glyph['bitmap_top']
            del glyph['bitmap_top']  # Remove temporary field
        else:
            # Empty glyph (like space)
            glyph['y_offset'] = 0
    
    print(f"Actual glyph bounds: top={max_glyph_top}, bottom={max_glyph_bottom}")
    print(f"Minimal line height (just glyphs): {minimal_line_height}px")
    print(f"Standard line height would be: {ascender - descender}px")
    print(f"Adjusted baseline (from top): {adjusted_baseline}px")
    
    # Generate Rust code
    weight_info = f'//! Weight: {weight}\n' if weight is not None else ''
    rust_code = f'''//! Gray4 (4-bit, 16-level) anti-aliased font with minimal line height
//! Generated from: {Path(font_path).name}
//! Size: {font_size}px
{weight_info}//! Characters: {len(glyphs)}
//! Line height: {minimal_line_height}px (minimal - actual glyph bounds)

use super::gray4_font::{{GlyphInfo, Gray4Font}};

/// Packed pixel data (2 pixels per byte, 4 bits each)
pub const {output_name.upper()}_DATA: &[u8] = &[
'''
    
    # Write pixel data in hex format
    for i in range(0, len(packed_data), 16):
        chunk = packed_data[i:i+16]
        hex_bytes = ', '.join(f'0x{b:02x}' for b in chunk)
        rust_code += f'    {hex_bytes},\n'
    
    rust_code += f'''];

/// Glyph metadata for binary search
pub const {output_name.upper()}_GLYPHS: &[GlyphInfo] = &[
'''
    
    # Write glyph metadata
    for glyph in glyphs:
        # Properly escape special characters
        if glyph['char'] == '\\':
            char_repr = '\\\\'
        elif glyph['char'] == "'":
            char_repr = "\\'"
        elif ord(glyph['char']) < 32 or ord(glyph['char']) > 126:
            char_repr = f'\\u{{{ord(glyph["char"]):04x}}}'
        else:
            char_repr = glyph['char']
        
        rust_code += f'''    GlyphInfo {{
        character: '{char_repr}',
        width: {glyph['width']},
        height: {glyph['height']},
        x_offset: {glyph['x_offset']},
        y_offset: {glyph['y_offset']},
        x_advance: {glyph['x_advance']},
        data_offset: {glyph['data_offset']},
    }},
'''
    
    rust_code += f'''];

/// The font
pub const {output_name.upper()}: Gray4Font = Gray4Font {{
    baseline: {adjusted_baseline},
    line_height: {minimal_line_height},
    packed_data: {output_name.upper()}_DATA,
    glyphs: {output_name.upper()}_GLYPHS,
}};
'''
    
    return rust_code, len(packed_data)

def main():
    parser = argparse.ArgumentParser(description='Generate Gray4 anti-aliased fonts with minimal line height')
    parser.add_argument('font_path', type=Path, help='Path to TrueType/OpenType font file')
    parser.add_argument('size', type=int, help='Font size in pixels')
    parser.add_argument('output_name', help='Output constant name (e.g., noto_sans_24_regular)')
    parser.add_argument('--weight', type=int, default=None,
                        help='Font weight for Variable fonts (100-900, default: 400). Common values: 300=Light, 400=Regular, 700=Bold')

    args = parser.parse_args()

    if not args.font_path.exists():
        print(f"Error: Font file not found: {args.font_path}", file=sys.stderr)
        sys.exit(1)

    # Default weight to 400 if not specified
    weight = args.weight if args.weight is not None else 400

    # Generate the font
    rust_code, total_size = generate_gray4_font(args.font_path, args.size, args.output_name, weight=weight)

    # Create generated/ directory if it doesn't exist
    generated_dir = Path("generated")
    generated_dir.mkdir(exist_ok=True)

    # Write to generated/ directory
    output_path = generated_dir / f"{args.output_name}.rs"
    output_path.write_text(rust_code)

    print(f"Generated {output_path}")
    print(f"Total data size: {total_size} bytes")
    print(f"\nTo use this font, copy it to ../src/ and add it to ../src/lib.rs")

if __name__ == '__main__':
    main()