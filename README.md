# MEGA Structured Downloader

A reliable MEGA downloader that **preserves folder structure** (and can export
zips), using **Real-Debrid** as the byte source to sidestep MEGA's 5 GB cap.

> **Requirement:** a **Real-Debrid premium subscription** (you paste your API
> token in the app's Settings). Real-Debrid is the primary byte transport and
> what bypasses MEGA's transfer limits. There is a native-MEGA fallback for
> files Real-Debrid can't serve (see below), but it downloads directly from
> MEGA and is therefore subject to MEGA's normal quota — without an RD token
> this tool loses its main advantage.

## Install (Windows)

Grab the latest installer from the
[Releases page](https://github.com/greatgreatasset/mega-downloader/releases)
and run it.

> **Windows SmartScreen warning:** the installer is **not code-signed**, so
> Windows will show *"Windows protected your PC"* the first time you run it.
> Click **More info → Run anyway**. This is expected for unsigned open-source
> software — if you'd rather not trust a prebuilt binary, build it yourself
> from source (instructions below).

This software is provided as-is, with no warranty or support.

## Purpose

The installer ships a desktop app that does two things existing tools don't do
*together*:

1. **Structured (foldered) downloads.** Given a MEGA folder link, it
   reconstructs the exact folder tree and writes every file into its correct
   nested directory on disk — no flattening, and optional structure-preserving
   zip export.
2. **Bypasses MEGA's download limits.** Bytes are fetched through Real-Debrid
   instead of directly from MEGA, sidestepping the ~5 GB free-transfer cap.
   Verified on a 14 GB / 83-file folder in one run.

## Why

- **Real-Debrid** beats the limit but flattens every folder into one big list.
- **MegaBasterd** keeps structure but breaks constantly.

This tool separates the two problems: MEGA folder *listing* is free and
unmetered, so we always know the correct tree (the "structure brain"), then pull
the actual bytes from Real-Debrid and **re-fold them into the exact directories**.

When Real-Debrid can't serve a file (its MEGA cache is cold for that node), the
engine falls back to downloading that one file **natively from MEGA** — this
keeps a whole-folder run from failing on a couple of stragglers, but those
fallback bytes do count against MEGA's quota.

## Architecture

```
UI (React, localhost)  ──REST + WebSocket──►  Engine (Rust, headless)
                                              ├─ mega        structure brain (tree, keys)
                                              ├─ realdebrid  byte source
                                              ├─ db          SQLite, restart-safe queue
                                              └─ server      axum REST + WS
```

The engine runs as a standalone process so downloads survive a UI crash/reload.
Phase 6 wraps it as a Tauri sidecar for a one-click desktop app.

## Layout

| Path | What |
|------|------|
| `crates/engine` | Core library: MEGA parsing/crypto, RD client, DB, models |
| `crates/server` | `mega-downloader` binary: axum REST + WebSocket |
| `migrations`    | SQLite schema |
| `ui`            | Vite + React + Tailwind frontend |

## Develop (web)

Two processes — the engine and the UI:

```bash
# Terminal 1 — engine on http://127.0.0.1:8787
cargo run -p server

# Terminal 2 — UI on http://localhost:5173 (proxies /api + /ws to the engine)
cd ui && npm install && npm run dev
```

Open http://localhost:5173. Paste a MEGA folder link, Inspect, then Download.
Downloads go to `./downloads` (override with `DOWNLOAD_DIR`); state lives in
`./mega-downloader.db` (override with `DB_PATH`).

## Desktop app (Tauri)

The same engine binary is wrapped as a **Tauri sidecar** — it runs as a separate
process the app spawns on launch (and kills on exit), so downloads survive a UI
crash. The window loads the built UI, which talks to the sidecar on `:8787`.

```bash
npm install                 # root: installs the Tauri CLI

# Dev (hot-reload UI + app window):
npm run tauri dev

# Build an installer (NSIS .exe):
#   1) build the engine and stage it as the sidecar
cargo build -p server --release
cp target/release/mega-downloader.exe \
   src-tauri/binaries/mega-downloader-x86_64-pc-windows-msvc.exe
#   2) bundle
npm run tauri build
# → installer under src-tauri/target/release/bundle/nsis/
```

In the packaged app the DB lives in the OS app-data dir and downloads default to
`<Downloads>/MegaDownloader`.

## Did this need to be an installed app? (honest answer: no)

The core job — "give me a MEGA link, rebuild the tree, pull the bytes, resume
if interrupted" — is a classic command-line shape. The installer is a
convenience choice, not an architectural necessity, and the codebase was
deliberately layered so the alternatives stay cheap:

- **`crates/engine`** is a plain Rust library (MEGA parsing/crypto, Real-Debrid
  client, SQLite queue). It has no UI or server dependencies.
- **`crates/server`** is a standalone headless binary with a REST + WebSocket
  API. You can run it today with `cargo run -p server` and drive it entirely
  over HTTP — no window, no installer.
- The **Tauri app is just a thin shell** that spawns that same binary as a
  sidecar and points a WebView at the bundled UI.

What that means in practice:

- **A CLI would be one small crate away.** A `crates/cli` frontend
  (`mega-dl <link> --dest <dir>` with progress bars) would reuse the engine
  library directly — no Tauri, no WebView2, no CORS, no installer, and a much
  smaller build. It just hasn't been needed yet.
- **A portable single .exe is a packaging option, not a redesign.** The NSIS
  installer is simply Tauri's default Windows bundle; Tauri can emit a portable
  executable instead if installation itself is the annoyance. (The headless
  `mega-downloader.exe` server binary is *already* portable on its own.)

So why ship the GUI installer as the primary artifact? The value shows up on
long runs: watching per-file progress across dozens of files, pausing/resuming/
deleting individual jobs, browsing the reconstructed tree *before* committing
to a download, and a persistent job list that survives restarts. All of that is
possible in a terminal but genuinely nicer in a window — and since this tool
exists to replace MegaBasterd (a GUI app), a desktop app matched the intent.
If your workflow is single-shot scripted downloads, the headless server (or a
future CLI crate) is the better fit, and nothing in the design forecloses it.

## Roadmap

All phases complete and verified end-to-end (14 GB / 83-file real-world run).

- **Phase 0** — scaffold + engine↔UI handshake ✅
- **Phase 1** — structure brain: parse links, reconstruct + render the tree; RD per-node-link spike ✅
- **Phase 2** — RD download engine: stream bytes into the correct nested folders ✅
- **Phase 3** — reliability: retries, error handling, crash recovery, integrity, native-MEGA fallback ✅
- **Phase 4** — zip export ✅
- **Phase 5** — queue + UX polish (deferred retries, pause/resume/delete, job persistence) ✅
- **Phase 6** — Tauri packaging (sidecar engine, NSIS installer) ✅

## License

[MIT](LICENSE)
