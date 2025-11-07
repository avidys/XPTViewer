# XPTViewer

A Tauri + Svelte desktop experience inspired by the classic **xpt4mac** utility. XPTViewer lets you
inspect SAS XPORT (`.xpt`/`.xport`) files, explore dataset metadata, and preview observation data in a
modern, cross-platform interface.

## Getting Started

```bash
npm install
npm run tauri dev
```

The development command spins up the Vite dev server and launches the Tauri shell.

To produce a release build:

```bash
npm run build
npm run tauri build
```

## Features

- Native desktop shell powered by Tauri
- Svelte front-end with a polished card-based layout
- Open SAS XPORT files through the system file picker
- Inspect dataset metadata, including labels and observation counts
- Preview up to the first 100 rows for each dataset with automatic type-aware formatting

## Project Structure

```
/
├── index.html              # Vite entry point
├── src                     # Svelte application
├── src-tauri               # Tauri (Rust) backend
└── tauri.conf.json         # Tauri configuration
```

## Notes

- The preview intentionally limits to the first 100 observations per dataset to keep rendering
  responsive.
- Numeric values retain their precision; trailing zeros are trimmed for readability.
