# RustifiedYoutube

An unofficial Windows desktop client for YouTube Music, built with Rust, Tauri 2, and Microsoft Edge WebView2.

## What it does

- Opens the YouTube Music website in a dedicated desktop window.
- Keeps the WebView profile, cookies, and session data between launches.
- Keeps playing when the window is closed to the system tray.
- Provides native play/pause, previous, next, and mute controls.
- Restores window size and position and brings the existing window forward when launched again.
- Keeps sign-in redirects inside the WebView; opens unrelated HTTPS popup links in the default browser.

The first version uses YouTube Music's web interface. It does not implement a separate streaming API, offline downloads, or automatic updates. Google sign-in and account-specific features need to be verified manually with your account; embedded-browser login is subject to Google's policies.

## Running locally

Requires Windows 10/11 x64, [Rust](https://rustup.rs/), [Microsoft C++ Build Tools](https://v2.tauri.app/start/prerequisites/#windows), Node.js 24 LTS, and WebView2 Runtime. Restart your terminal after installing Rust so `cargo` is on PATH.

```powershell
npm ci
npm run dev
```

## Controls

Find the running version in **Help → About RustifiedYoutube**. The menu label and About dialog use the application's build version automatically.

Shortcuts work while the application is focused. Track controls operate on the YouTube Music player after a track has been selected.

| Action | Shortcut |
| --- | --- |
| Play / pause | Ctrl + Alt + Space |
| Next / previous | Ctrl + Alt + Right / Left |
| Mute / unmute | Ctrl + Alt + M |
| Back / forward | Alt + Left / Right |
| Home | Ctrl + H |
| Reload | Ctrl + R |
| Hide to tray | Ctrl + W, or close the window |
| Quit completely | Ctrl + Q, or tray menu → Quit |

Click the tray icon to reopen the window; right-click it for playback controls. Closing the window keeps the application running.

## Build and checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm test
npm run build
./scripts/package.ps1
```

The installer, portable ZIP, and SHA-256 checksums are collected in `dist/`. The installer sets up WebView2 if needed. The portable executable requires WebView2 to be installed already. Builds are currently unsigned.

## Releases

[GitHub Releases](https://github.com/zygerr/RustifiedYoutube/releases) hosts installers and release notes. While this repository is private, only people with repository access can download its releases.

The Windows workflow checks and builds every push to `main` and every pull request. To prepare a release, update the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and their lockfiles, then push a matching tag such as `v0.1.0`. A successful tagged build creates a **draft** GitHub Release containing the installer, portable ZIP, and checksums. Review it before publishing.

## Data and permissions

The dedicated WebView2 profile is stored under `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\webview`. Window state is stored in the app's configuration directory. Uninstalling the executable does not necessarily remove the profile.

Remote pages receive no Tauri capabilities or custom native commands. Playback actions are initiated by the native menu and only run on the exact `https://music.youtube.com` origin. Credentials are entered on Google's own pages; the application does not copy another browser's cookies.

This project is independent and is not affiliated with Google or YouTube.
