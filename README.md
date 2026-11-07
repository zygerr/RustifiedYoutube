# RustifiedYoutube

An unofficial Windows x64 YouTube Music client. The interface, search, library, queue, settings, and playback controls are written in Rust with egui and wgpu/DirectX 12. Version 0.3 uses the official YouTube Music player inside a persistent WebView2 media surface for direct audio/video streaming.

## Playback and interface

- Browse a YouTube Music inspired native interface with dark surfaces, a persistent search field, mood filters, Listen again, Quick picks, and Discover more.
- See consistently sized artwork: 180 px square catalog cards and 48 px covers in 64 px song rows. Wide video thumbnails are center-cropped instead of changing the layout or stretching. Long titles truncate within fixed card dimensions.
- Load sharper artwork at up to 512 px for browsing and 1536 px for the expanded player, with HD video-thumbnail fallbacks where available. The smaller cover stays visible while HD loads. The decoded artwork cache has a 64 MiB budget; small source images are never artificially enlarged.

- Start streaming without downloading and remuxing an entire track first. The YouTube engine warms up when the app opens and stays alive between tracks. Actual startup time still depends on YouTube, the connection, and any normal playback restrictions.
- Click the current song or artwork in the bottom-left player bar to open the expanded player.
- The bottom song information, controls, and seek rail stay in the same position when opening or closing the expanded player.
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

Choose **Continue with Google** to connect. The app first reuses the account already saved in its own WebView2 profile; a Google window opens only when interactive sign-in is necessary. Complete any Google challenges yourself. **Settings → Account → Refresh connection** rechecks the current profile without reloading playback.

The native library session is synchronized from the browser at startup and every five minutes. Rejected authenticated requests trigger a coordinated refresh and one retry. Temporary connection or server errors keep the saved account, and public search remains available if Google requires a new sign-in. Google can still expire or revoke a login; the client cannot prevent that.

The player and temporary sign-in window reuse only this application's existing dedicated WebView2 profile. Search can also work without connecting an account; playback remains subject to YouTube's availability, account requirements, and normal advertisements. Popup navigation is disabled in the media surface; use the separate sign-in window for authentication.

## Controls

Use the native player bar or tray menu. `Ctrl+Alt+Space` toggles playback and `Ctrl+Q` exits, including when the video surface has focus. Closing the window keeps playback in the tray by default; change that in Settings. Click the tray icon to restore the window. The version is visible in the sidebar, Settings, About, and tray tooltip.

Scroll up/down over the volume slider or percentage. **Settings → Playback → Volume step per scroll notch** accepts an integer from **1% to 100%**, with a default of 5%. Volume remains within 0–100%, and the step is saved across launches.

The same scroll step also works over the current-song thumbnail in the footer and the large artwork in the expanded Audio player. Hovering the large cover shows the volume percentage. Scrolling over it adjusts volume without scrolling the page or the upcoming queue.

The saved volume is applied before playback starts and limits new audio/video sources immediately. Lower levels used by YouTube's loudness normalization are preserved.

## Discord Rich Presence

Keep Discord desktop running and play a song. Rich Presence shows the song, artist, cover, progress, and a **Listen on YouTube Music** button. **Settings → Discord Rich Presence** enables or disables sharing and shows the connection status. Presence is cleared on pause, buffering, advertisements, or exit, and the client reconnects when Discord restarts.

The default public Application ID is `1568660934296076398`. You can override it under **Application settings** using an application registered in the [Discord Developer Portal](https://discord.com/developers/applications). This uses Discord's [local RPC protocol](https://docs.discord.com/developers/topics/rpc); no account login, bot token, or client secret is needed. YouTube/Google session data never enters the RPC payload. Discord's own activity-sharing settings must also allow the activity to be visible.

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
./target/debug/RustifiedYoutube.exe --smoke-rpc
./target/debug/RustifiedYoutube.exe --smoke-account
./target/debug/RustifiedYoutube.exe --smoke-artwork
```

The streaming check opens a temporary app instance with volume zero, measures two real streams, verifies video dimensions, pause, seek confirmation, and Audio/Video switching, then closes. It does not change saved UI preferences. Unit tests cover full title and history parsing, artist ranking, styled-title glyph coverage, volume wheel interaction, seek interaction and width, shuffle order, and media bridge restrictions.

The RPC check verifies the real Discord handshake and acknowledgement of a clear-activity command without displaying a test song. Unit tests also cover presence timestamps, pause/ad clearing, Unicode limits, fragmented IPC frames, pings, and immediate volume protection.

The account check restores the current profile, reads library/history counts without logging their names, refreshes the connection, and exits. It never opens an interactive Google login, starts music, publishes Discord presence, or changes saved UI preferences. Running it again checks restoration across app restarts. The streaming check also refreshes an existing account during playback and verifies that the current track and position are preserved.

The artwork check fetches public album/video images, verifies that the expanded images retain more resolution than browsing images, and prints dimensions only. Unit tests cover high-resolution source selection, URL fallbacks, bounded decoding, and artwork wheel behavior alongside the volume slider.

Debug builds also support offline visual snapshots:

```powershell
./target/debug/RustifiedYoutube.exe --ui-snapshot discover
./target/debug/RustifiedYoutube.exe --ui-snapshot library --compact
```

Supported pages are `discover`, `library`, `search`, `queue`, `player`, and `settings`. Snapshots use synthetic artwork and catalog fixtures, skip metadata/account/media connections and Discord sharing, and save PNGs plus isolated test preferences under `target/ui-preview/`. The normal snapshot window is 1440 × 900; `--compact` uses 1080 × 700. The app closes after capture. This mode is disabled in release builds.

## Releases

The [Windows workflow](https://github.com/zygerr/RustifiedYoutube/actions/workflows/windows.yml) checks and builds pushes to `main` and pull requests. Change the workspace version in `Cargo.toml`, update `Cargo.lock`, and push a matching tag to create a **draft** [GitHub Release](https://github.com/zygerr/RustifiedYoutube/releases) with installer, portable ZIP, and checksums. Private releases require repository access.

## Local data

The dedicated Google/WebView2 profile remains at `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\webview`. The metadata API session is stored alongside it as `session.dpapi`, encrypted with Windows DPAPI for the current Windows user and updated atomically. Version 0.3.4 also saves the profile's browser User-Agent and reads the previous session format automatically. The application never copies another browser's profile. Session cookies are read through native WebView2 APIs, including HttpOnly cookies, and never pass through JavaScript IPC, logs, or Discord. Do not share these files.

**Disconnect native session** removes the encrypted metadata API session and writes `session.disabled` to stop automatic native reconnection. It does not sign out the separate Google/WebView2 profile. **Continue with Google** enables reconnection and can restore that account. Uninstalling preserves user data. UI preferences are stored in eframe's per-user application directory. Old `audio-buffer` files from earlier versions are not used by version 0.3.

The streaming architecture was researched against [ytmdesktop's integration with the official player](https://github.com/ytmdesktop/ytmdesktop/blob/development/src/renderer/ytmview/preload.ts). RustifiedYoutube has its own Rust interface and media adapter. This project is independent and is not affiliated with Google or YouTube.
