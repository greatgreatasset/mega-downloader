# MEGA Structured Downloader

A Windows desktop app that downloads MEGA folder links with the **folder
structure fully preserved**, using **Real-Debrid** as the download source to
avoid MEGA's ~5 GB free-transfer cap.

Existing tools make you pick one or the other:

- **Real-Debrid** gets past the transfer cap, but flattens every folder into
  one big list of files.
- **MegaBasterd** keeps the folder structure, but is unreliable.

This tool does both: it reads the folder tree directly from MEGA (folder
*listing* is unmetered), fetches the file bytes through Real-Debrid, and
writes every file into its correct nested directory — with pause/resume,
automatic retries, crash-safe resumable downloads, integrity checks, and
optional structure-preserving zip export.

When Real-Debrid can't serve a file, the engine falls back to downloading that
file directly from MEGA so a whole-folder run doesn't fail on a few
stragglers. Fallback downloads do count against MEGA's normal transfer quota.

## Requirements

- **A Real-Debrid premium subscription.** Paste your API token in the app's
  Settings. Without it the tool loses its main advantage, since only the
  native-MEGA fallback would remain.
- Windows 10/11, x64.

## Install

Download the latest installer from the
[Releases page](https://github.com/greatgreatasset/mega-downloader/releases)
and run it.

> **Windows SmartScreen warning:** the installer is **not code-signed**, so
> Windows will show *"Windows protected your PC"* the first time you run it.
> Click **More info → Run anyway**. This is expected for unsigned open-source
> software — if you'd rather not trust a prebuilt binary, build it yourself
> from source (instructions below).

This software is provided as-is, with no warranty or support.

## Usage

1. Open the app and paste your Real-Debrid API token in **Settings**.
2. Paste a MEGA folder link and click **Inspect** to preview the folder tree.
3. Click **Download**. Files land in `<Downloads>/MegaDownloader` by default
   (configurable in Settings), in their original nested folders.

Jobs persist across restarts and can be paused, resumed, or deleted
individually. A finished job can be exported as a zip that preserves the
folder structure.

## How it works

```
UI (React, localhost)  ──REST + WebSocket──►  Engine (Rust, headless)
                                              ├─ mega        folder tree + keys
                                              ├─ realdebrid  byte source
                                              ├─ db          SQLite, restart-safe queue
                                              └─ server      axum REST + WS
```

The engine runs as a separate process (a Tauri sidecar in the packaged app),
so downloads survive a UI crash or reload. State lives in SQLite; interrupted
downloads resume from where they left off.

| Path | What |
|------|------|
| `crates/engine` | Core library: MEGA parsing/crypto, Real-Debrid client, DB |
| `crates/server` | `mega-downloader` binary: axum REST + WebSocket API |
| `migrations`    | SQLite schema |
| `ui`            | Vite + React + Tailwind frontend |
| `src-tauri`     | Tauri desktop shell |

## Building from source

Prerequisites: Rust (MSVC toolchain) and Node.js.

**Run the web version (development):**

```bash
# Terminal 1 — engine on http://127.0.0.1:8787
cargo run -p server

# Terminal 2 — UI on http://localhost:5173
cd ui && npm install && npm run dev
```

**Build the desktop installer:**

```bash
npm install                 # root: installs the Tauri CLI

# 1) build the engine and stage it as the Tauri sidecar
cargo build -p server --release
cp target/release/mega-downloader.exe \
   src-tauri/binaries/mega-downloader-x86_64-pc-windows-msvc.exe

# 2) bundle → installer under src-tauri/target/release/bundle/nsis/
npm run tauri build
```

The headless engine can also be used on its own: run
`cargo run -p server` and drive it over its REST + WebSocket API
(state in `./mega-downloader.db`, downloads to `./downloads`; override with
`DB_PATH` / `DOWNLOAD_DIR`).

## License

[MIT](LICENSE)
