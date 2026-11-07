# RustifiedYoutube

An unofficial Windows x64 YouTube Music client. The interface, search, library, queue, settings, and playback controls are written in Rust with egui and wgpu/DirectX 12. Version 0.3 uses the official YouTube Music player inside a persistent WebView2 media surface for direct audio/video streaming.

## Playback and interface

- Start streaming without downloading and remuxing an entire track first. The YouTube engine warms up when the app opens and stays alive between tracks. Actual startup time still depends on YouTube, the connection, and any normal playback restrictions.
- Click the current song or artwork in the bottom-left player bar to open the expanded player.
- Switch between Audio (artwork) and Video in the expanded view. When YouTube Music offers separate song/video versions, the player uses its own toggle; a track without a music video cannot gain one.
- View and play upcoming queue entries in the right-hand panel. Back returns to search or the library without stopping playback.
- Render styled Unicode song titles, symbols, and emoji with fallback fonts installed by Windows. Windows font files are read locally and are not redistributed.
- Search both songs and videos, filter All/Songs/Videos, and prioritize matching artists while keeping full titles.
- Connect a Google account to access playlists, liked songs, and like/unlike actions.
- Find account listening history and locally remembered plays in **Discover → Listen again**. Click a card to play; Refresh reloads account history.
- Seek on the full-width bottom rail or the rail below the expanded media panel. Click or drag to seek; focus the rail and use Left/Right to move five seconds.
- Play, pause, change volume, use previous/next, repeat the queue, and keep playing in the tray. **Shuffle** randomizes upcoming tracks while preserving the current song and played entries; switching it off restores the remaining original order. The choice is saved across launches.

The WebView is active during playback. It shows only the media surface in Video mode; the surrounding interface remains Rust. yt-dlp, Node.js, FFmpeg, complete-track downloads, and the old temporary audio cache are no longer used or bundled. There is no offline library or automatic updater.

## Running locally

Requires Windows 10/11 x64, a DirectX 12 compatible graphics driver, Rust stable with the MSVC toolchain, Microsoft C++ Build Tools, and [Microsoft WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/). WebView2 is required for audio/video playback as well as sign-in.

```powershell
cargo build --workspace --locked
cargo run -p rustified-youtube-native --locked
```

Choose **Connect Google** to sign in or refresh the account. Complete any Google challenges yourself. The player and temporary sign-in window reuse only this application's existing dedicated WebView2 profile. Search can also work without connecting an account; playback remains subject to YouTube's availability, account requirements, and normal advertisements. Popup navigation is disabled in the media surface; use the separate sign-in window for authentication.

## Controls

Use the native player bar or tray menu. `Ctrl+Alt+Space` toggles playback and `Ctrl+Q` exits, including when the video surface has focus. Closing the window keeps playback in the tray by default; change that in Settings. Click the tray icon to restore the window. The version is visible in the sidebar, Settings, About, and tray tooltip.

Scroll up/down over the volume slider or percentage. **Settings → Playback → Volume step per scroll notch** accepts an integer from **1% to 100%**, with a default of 5%. Volume remains within 0–100%, and the step is saved across launches.

## Build and checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test native/tests/media_bridge.cjs
./scripts/build-native.ps1
```

The per-user installer, portable ZIP, and SHA-256 checksums are written to `dist/<version>/`. Extract the whole portable ZIP before launching `RustifiedYoutube.exe`; its adjacent sign-in helper is required. Builds are currently unsigned. The packaging script downloads only a pinned, checksum-verified NSIS build tool.

Node.js 24 is used only to run the media bridge regression tests during development and CI; it is not required or bundled for playback.

Optional live checks (these contact YouTube):

```powershell
cargo run -p rustified-core --example smoke -- --authenticated
./target/debug/RustifiedYoutube.exe --smoke-stream
```

The streaming check opens a temporary app instance with volume zero, measures two real streams, verifies video dimensions, pause, seek confirmation, and Audio/Video switching, then closes. It does not change saved UI preferences. Unit tests cover full title and history parsing, artist ranking, styled-title glyph coverage, volume wheel interaction, seek interaction and width, shuffle order, and media bridge restrictions.

## Releases

The [Windows workflow](https://github.com/zygerr/RustifiedYoutube/actions/workflows/windows.yml) checks and builds pushes to `main` and pull requests. Change the workspace version in `Cargo.toml`, update `Cargo.lock`, and push a matching tag to create a **draft** [GitHub Release](https://github.com/zygerr/RustifiedYoutube/releases) with installer, portable ZIP, and checksums. Private releases require repository access.

## Local data

The dedicated Google/WebView2 profile remains at `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\webview`. The metadata API session is stored alongside it as `session.dpapi`, encrypted with Windows DPAPI for the current Windows user. The application never copies another browser's profile. Do not share these files.

**Disconnect native session** removes the encrypted metadata API session. It does not sign out the separate Google/WebView2 profile; reconnecting can restore that account. Uninstalling preserves user data. UI preferences are stored in eframe's per-user application directory. Old `audio-buffer` files from earlier versions are not used by version 0.3.

The streaming architecture was researched against [ytmdesktop's integration with the official player](https://github.com/ytmdesktop/ytmdesktop/blob/development/src/renderer/ytmview/preload.ts). RustifiedYoutube has its own Rust interface and media adapter. This project is independent and is not affiliated with Google or YouTube.
