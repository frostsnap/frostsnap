# VecFramebuffer

A Vec-based framebuffer implementation that provides dynamic sizing, as opposed to the const generic approach used by embedded-graphics' built-in Framebuffer.

## Features

- **Dynamic Sizing**: Width and height are runtime values, not compile-time constants
- **Multiple Color Formats**: Supports Rgb565, Gray8, Gray4, Gray2, and BinaryColor
- **Efficient Pixel Packing**: Optimized bit manipulation for sub-byte pixel formats
- **DrawTarget Implementation**: Compatible with embedded-graphics drawing operations

## Usage

```rust
use frostsnap_embedded_widgets::vec_framebuffer::VecFramebuffer;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
};

// Create a 240x280 framebuffer for Rgb565 colors
let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(240, 280);

// Set individual pixels
fb.set_pixel(Point::new(10, 10), Rgb565::RED);

// Draw shapes using embedded-graphics
let circle = Circle::new(Point::new(50, 50), 30)
    .into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN));
circle.draw(&mut fb).unwrap();

// Clear the entire framebuffer
fb.clear(Rgb565::BLACK);

// Fill a rectangular region
let rect = Rectangle::new(Point::new(5, 5), Size::new(20, 20));
fb.fill_rect(rect, Rgb565::GREEN);
```

## Implementation Details

### Pixel Storage

The implementation closely follows embedded-graphics' framebuffer, with pixels packed efficiently:

- **16-bit colors (Rgb565)**: 2 bytes per pixel, stored in little-endian byte order
- **8-bit colors (Gray8)**: 1 byte per pixel, direct mapping
- **4-bit colors (Gray4)**: 2 pixels per byte, nibble-packed
- **2-bit colors (Gray2)**: 4 pixels per byte
- **1-bit colors (BinaryColor)**: 8 pixels per byte, MSB first

### Direct Method Implementation

Each color type has its methods implemented directly on `VecFramebuffer<ColorType>` using macros:

- `set_pixel(point, color)`: For writing pixels
- `get_pixel(point) -> Option<Color>`: For reading pixels
- `clear(color)`: For clearing the entire framebuffer
- `fill_rect(rect, color)`: For filling rectangular regions

This approach follows embedded-graphics patterns while providing specialized implementations for each color type.

## Performance Considerations

The current implementation is a faithful translation of embedded-graphics' framebuffer. Several optimizations are possible:

1. **Bulk Operations**: Use `slice::copy_from_slice` for aligned rectangular fills
2. **SIMD**: Leverage SIMD instructions for batch pixel operations
3. **Lookup Tables**: Pre-computed tables for bit manipulation in sub-byte formats
4. **Memory Alignment**: Ensure buffer alignment for better cache performance
5. **Unsafe Variants**: Skip bounds checking in hot paths where safety is guaranteed
6. **Optimized Clear**: Use `memset` for common colors (all black/white)

## Differences from embedded-graphics Framebuffer

| Feature | embedded-graphics | VecFramebuffer |
|---------|------------------|----------------|
| Size specification | Const generics | Runtime values |
| Memory allocation | Stack (array) | Heap (Vec) |
| Size changes | Not possible | Can resize (with new allocation) |
| Compile-time size checking | Yes | No |
| Binary size | Larger (per size) | Smaller |

## Future Enhancements

- Add support for more color formats (Rgb888, etc.)
- Implement `as_image()` for compatibility with embedded-graphics Image
- Add `blit` operations for copying between framebuffers
- Implement double buffering support
- Add DMA-friendly memory alignment options