# RustifiedYoutube

An unofficial YouTube Music desktop client for Windows x64. Version 0.2 uses a native Rust interface built with egui and wgpu (DirectX 12), with audio decoded by Windows MediaPlayer.

## Features

- Search songs directly through the YouTube Music metadata API.
- Connect your Google account to open playlists and liked songs, and like/unlike tracks.
- Play, pause, seek, change volume, use previous/next, repeat the queue, and add songs to the queue.
- Keep playing in the system tray, reopen the existing window, and save volume preferences.
- Find the build version in the sidebar, Settings, About, and tray tooltip.

The main window contains no HTML, JavaScript frontend, or WebView. A separate WebView2 helper opens temporarily for Google sign-in, then exits. Playback preparation uses bundled yt-dlp and Node.js helper processes; the application is not composed exclusively of Rust binaries. Google authentication and the unofficial YouTube API can change independently of the application.

Tracks are prepared in temporary audio buffers before native playback starts, so selecting a track can take several seconds depending on YouTube and the network. Buffers are removed when their owner is released normally. A forced process termination can leave temporary files under the app's `audio-buffer` directory. There is no offline library or automatic updater.

## Running locally

Requires Windows 10/11 x64, a DirectX 12 compatible graphics driver, Rust stable with the MSVC toolchain, and Microsoft C++ Build Tools. WebView2 Runtime is required only to connect or refresh a Google account.

```powershell
./scripts/fetch-native-tools.ps1
cargo build --workspace --locked
Copy-Item target/native-tools/yt-dlp.exe,target/native-tools/node.exe target/debug/
cargo run -p rustified-youtube-native --locked
```

Choose **Connect Google** to sign in. The helper reuses only this app's existing profile from earlier versions. Complete any Google challenges yourself. Search and playback are also available without signing in when YouTube permits the selected content.

## Controls

Use the player bar or the tray menu. While the window is focused, `Ctrl+Alt+Space` toggles playback and `Ctrl+Q` exits. Closing the window keeps playback in the tray by default; change that behavior in Settings. Click the tray icon to show the window again. `Ctrl+R` has no browser reload action in the native client.

## Build and checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
./scripts/build-native.ps1
```

The per-user installer, portable ZIP, and SHA-256 checksums are written to `dist/<version>/`. Extract the whole portable ZIP before launching `RustifiedYoutube.exe`; its adjacent helper binaries are required. Builds are currently unsigned. Build scripts download pinned helper versions from their official releases and verify executable/archive SHA-256 checksums.

Optional live checks (these contact YouTube):

```powershell
cargo run -p rustified-core --example smoke -- --authenticated
./target/debug/RustifiedYoutube.exe --smoke-audio
```

The second check prepares a public track and confirms that native playback advances with volume set to zero. Unit tests do not require a Google account.

## Releases

The [Windows workflow](https://github.com/zygerr/RustifiedYoutube/actions/workflows/windows.yml) checks and builds pushes to `main` and pull requests. Change the single workspace version in `Cargo.toml`, update `Cargo.lock`, and push a matching tag such as `v0.2.0` to create a **draft** [GitHub Release](https://github.com/zygerr/RustifiedYoutube/releases) with the installer, portable ZIP, and checksums. Private repository releases are accessible only to people who can access the repository.

## Local data

The dedicated Google sign-in profile is at `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\webview`. The native session is stored alongside it as `session.dpapi`, encrypted with Windows DPAPI for the current Windows user. Account cookies are sent only to YouTube for authenticated requests; a temporary cookie file is used by the playback helper and removed after preparation. Never share that directory or session files.

**Disconnect native session** removes the native encrypted session. It does not log out the separate Google sign-in profile, so connecting again can restore the same account. To sign out of Google completely, use Google's sign-out flow in that profile. Uninstalling preserves user data.

The native process stores UI preferences in eframe's per-user application directory. See `THIRD-PARTY-NOTICES.md` for helper notices and source links.

This project is independent and is not affiliated with Google or YouTube.
