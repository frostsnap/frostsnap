# Font Generator

Generate Gray4 (4-bit, 16-level) anti-aliased fonts for embedded systems.

## Setup

First time setup (run once):

```bash
cd frostsnap_fonts/font_tools

# Create virtual environment (creates venv/ directory)
python3 -m venv venv

# Activate it
source venv/bin/activate  # Linux/Mac
# OR
venv\Scripts\activate     # Windows

# Install dependencies
pip install -r requirements.txt
```

For subsequent uses, just activate the venv:
```bash
source venv/bin/activate  # Linux/Mac
```

## Usage

```bash
python generate_font.py <font.ttf> <size> <output_name> [--weight WEIGHT]
```

### Arguments

- `font.ttf` - Path to TrueType/OpenType font file
- `size` - Font size in pixels
- `output_name` - Name for the output file (e.g., noto_sans_18_light)
- `--weight` - Font weight for Variable fonts (100-900, default: 400)

### Weight Values

For Variable fonts, you can specify different weights:
- `100` - Thin
- `300` - Light
- `400` - Regular (default)
- `500` - Medium
- `700` - Bold
- `900` - Black

### Examples

Using the included fonts:

```bash
# Generate Noto Sans Light at 18px
python generate_font.py fonts/NotoSans-Variable.ttf 18 noto_sans_18_light --weight 300

# Generate Noto Sans Bold at 24px
python generate_font.py fonts/NotoSans-Variable.ttf 24 noto_sans_24_bold --weight 700

# Generate Noto Sans Mono Regular at 18px (weight defaults to 400)
python generate_font.py fonts/NotoSansMono-Variable.ttf 18 noto_sans_mono_18_regular

# Generate Noto Sans Medium at 18px
python generate_font.py fonts/NotoSans-Variable.ttf 18 noto_sans_18_medium --weight 500
```

Fonts are generated to `generated/` directory. To use them:

```bash
# Review the generated font
cat generated/noto_sans_18_light.rs

# Copy to src directory
cp generated/noto_sans_18_light.rs ../src/

# Add to ../src/lib.rs:
# pub mod noto_sans_18_light;
# pub use noto_sans_18_light::NOTO_SANS_18_LIGHT;
```

## Included Fonts

- `fonts/NotoSans-Variable.ttf` - Variable font with all weights (Light, Regular, Medium, Bold, etc.)
- `fonts/NotoSansMono-Variable.ttf` - Monospace variable font with all weights

## Output

Generates a Rust file (e.g., `noto_sans_18_light.rs`) in the `generated/` directory containing:
- Packed pixel data (2 pixels per byte, 4 bits each)
- Glyph metadata (character, dimensions, offsets)
- Font constant ready to use with embedded-graphics

Generated fonts must be manually copied to `../src/` to be used in the frostsnap_fonts crate.

## Font Format

The Gray4 format provides:
- 16 levels of grayscale (4 bits per pixel)
- Minimal line height (tight glyph bounds)
- Binary searchable character lookup
- ~97% smaller than atlas-based formats
