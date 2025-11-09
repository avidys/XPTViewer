#!/bin/bash
# Setup icon for Tauri app
mkdir -p icons
cp /Users/jean/dev/xpt4mac/Assets/icon_xpt.png icons/icon.png
echo "Icon setup complete: $(ls -lh icons/icon.png | awk '{print $9, $5}')"

