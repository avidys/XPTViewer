#!/bin/bash
# Create a minimal 1x1 transparent PNG
# PNG signature + IHDR + IDAT + IEND chunks

printf '\x89PNG\r\n\x1a\n' > icon.png
# IHDR chunk (13 bytes): width=1, height=1, bit depth=8, color type=6 (RGBA), compression=0, filter=0, interlace=0
printf '\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89' >> icon.png
# IDAT chunk: minimal compressed data (1x1 RGBA pixel = 4 bytes + 1 filter byte = 5 bytes)
printf '\x00\x00\x00\x0cIDATx\x9cc\x00\x00\x00\x02\x00\x01\x00\x00\x05\x00\x01\x0d\n-\xdb' >> icon.png
# IEND chunk
printf '\x00\x00\x00\x00IEND\xaeB`\x82' >> icon.png

