#!/usr/bin/env python3
"""
Generate art-window.ico from the tray glyph.

This script reads the glyph pattern, renders it at multiple sizes with a
white rounded background and dark ink, then packs the PNGs into an ICO file.
Uses only Python 3 stdlib: struct, zlib, io, and array.
"""

import struct
import zlib
import io
import array
from pathlib import Path

# The 18x18 tray glyph: '#' is ink, ' ' is transparent
GLYPH = [
    "                  ",
    "                  ",
    " ################ ",
    " #              # ",
    " #  ##          # ",
    " # ####         # ",
    " #  ##          # ",
    " #              # ",
    " #              # ",
    " #      ##      # ",
    " #     ####     # ",
    " #    ######    # ",
    " #   ########   # ",
    " #  ##########  # ",
    " # ############ # ",
    " ################ ",
    "                  ",
    "                  ",
]

# Colors
INK = (0x1a, 0x1a, 0x1a, 0xff)  # Dark ink, opaque
BACKGROUND = (0xff, 0xff, 0xff, 0xff)  # White, opaque
TRANSPARENT = (0xff, 0xff, 0xff, 0x00)  # White, transparent


def render_glyph(size: int) -> bytes:
    """
    Render the glyph to an RGBA image of the given size.

    Scales the 18x18 glyph to the target size with nearest-neighbor sampling.
    Adds white rounded corners background where the glyph sits.
    """
    pixels = bytearray(size * size * 4)
    scale = size / len(GLYPH)

    # Pre-render the glyph center with rounded corners
    # The glyph itself is roughly 16x16 in an 18x18 space
    radius = size // 8  # Rounding radius
    center = size // 2

    for y in range(size):
        for x in range(size):
            # Sample from the glyph using nearest-neighbor
            glyph_x = int((x - size * 0.08) / scale)  # Slight offset for centering
            glyph_y = int((y - size * 0.08) / scale)

            pixel = TRANSPARENT

            # Check if we're within the glyph bounds
            if 0 <= glyph_x < len(GLYPH[0]) and 0 <= glyph_y < len(GLYPH):
                if GLYPH[glyph_y][glyph_x] != ' ':
                    pixel = INK
                else:
                    # Add white background within the glyph frame
                    if glyph_x >= 1 and glyph_x < len(GLYPH[0]) - 1:
                        pixel = BACKGROUND
                    if glyph_y >= 1 and glyph_y < len(GLYPH) - 1:
                        pixel = BACKGROUND

            # Apply rounded corners to the background tile
            dx = x - center
            dy = y - center
            dist_from_center = (dx * dx + dy * dy) ** 0.5

            # If pixel is white background, apply corner fading
            if pixel == BACKGROUND:
                # Check corner distance for rounding
                corner_dist = ((abs(x - 0) - radius) ** 2 + (abs(y - 0) - radius) ** 2) ** 0.5
                if x < radius and y < radius and corner_dist > 0:
                    # Top-left corner rounding
                    if ((x - radius) ** 2 + (y - radius) ** 2) > (radius ** 2):
                        pixel = TRANSPARENT

            offset = (y * size + x) * 4
            pixels[offset:offset + 4] = bytes(pixel)

    return bytes(pixels)


def write_png(width: int, height: int, rgba_data: bytes) -> bytes:
    """
    Write a PNG file from RGBA data using struct and zlib.

    Returns the complete PNG file as bytes.
    """
    png_data = io.BytesIO()

    # PNG signature
    png_data.write(b'\x89PNG\r\n\x1a\n')

    # IHDR chunk (image header)
    ihdr = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)  # 8-bit RGBA, no interlace
    png_data.write(struct.pack('>I', len(ihdr)))
    png_data.write(b'IHDR')
    png_data.write(ihdr)
    png_data.write(struct.pack('>I', zlib.crc32(b'IHDR' + ihdr) & 0xffffffff))

    # IDAT chunk (image data)
    # Prepare scanlines with filter bytes
    scanlines = b''
    for y in range(height):
        scanlines += b'\x00'  # No filter for this scanline
        row_start = y * width * 4
        scanlines += rgba_data[row_start:row_start + width * 4]

    compressed = zlib.compress(scanlines, 9)
    png_data.write(struct.pack('>I', len(compressed)))
    png_data.write(b'IDAT')
    png_data.write(compressed)
    png_data.write(struct.pack('>I', zlib.crc32(b'IDAT' + compressed) & 0xffffffff))

    # IEND chunk (end marker)
    png_data.write(struct.pack('>I', 0))
    png_data.write(b'IEND')
    png_data.write(struct.pack('>I', zlib.crc32(b'IEND') & 0xffffffff))

    return png_data.getvalue()


def write_ico(png_files: list[tuple[int, bytes]]) -> bytes:
    """
    Pack PNG files into an ICO file.

    Args:
        png_files: List of (size, png_bytes) tuples, sorted by size

    Returns the complete ICO file as bytes.
    """
    ico_data = io.BytesIO()

    # ICO header
    ico_data.write(struct.pack('<HHH', 0, 1, len(png_files)))  # Reserved, Type (1=ICO), Count

    # Build directory entries and collect PNG data
    png_offsets = []
    offset = 6 + len(png_files) * 16  # After header and directory

    for size, png_bytes in png_files:
        # Directory entry for each PNG
        # In ICO format, size 256 is represented as 0
        size_byte = 0 if size == 256 else size
        ico_data.write(bytes([size_byte, size_byte]))  # Width, height
        ico_data.write(bytes([0, 0]))  # Colors, reserved
        ico_data.write(struct.pack('<HH', 1, 1))  # Color planes, bits per pixel
        ico_data.write(struct.pack('<I', len(png_bytes)))  # Size in bytes
        ico_data.write(struct.pack('<I', offset))  # Offset
        offset += len(png_bytes)

    # Write PNG data
    for _, png_bytes in png_files:
        ico_data.write(png_bytes)

    return ico_data.getvalue()


def main():
    script_dir = Path(__file__).parent
    ico_path = script_dir / 'art-window.ico'

    sizes = [16, 24, 32, 48, 64, 128, 256]
    png_files = []

    print(f"Generating icon sizes: {sizes}")
    for size in sizes:
        print(f"  Rendering {size}x{size}...", end=" ", flush=True)
        rgba = render_glyph(size)
        print(f"PNG...", end=" ", flush=True)
        png_bytes = write_png(size, size, rgba)
        png_files.append((size, png_bytes))
        print(f"({len(png_bytes)} bytes)")

    print("Packing ICO...", end=" ", flush=True)
    ico_bytes = write_ico(png_files)
    print(f"({len(ico_bytes)} bytes)")

    ico_path.write_bytes(ico_bytes)
    print(f"Wrote {ico_path}")


if __name__ == '__main__':
    main()
