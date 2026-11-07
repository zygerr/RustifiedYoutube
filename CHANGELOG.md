# Changelog

## 0.2.0

- Replace the website wrapper with a native Rust interface using egui, wgpu, and DirectX 12.
- Search songs, open account playlists and liked songs, and manage the playback queue natively.
- Decode audio with Windows MediaPlayer; prepare tracks in temporary buffers using yt-dlp and Node.js.
- Keep WebView2 only in a separate, temporary Google sign-in process. Reuse the app's existing account profile and protect the native session with Windows DPAPI.
- Show version 0.2.0 in the sidebar, Settings, About, and tray tooltip.
- Retain tray playback, single-instance behavior, volume, seeking, and previous/next controls.
- Package the native client and its helpers in the Windows installer and portable ZIP.

## 0.1.2

- Show the running app version in Help → About RustifiedYoutube and its native About dialog.

## 0.1.1

- Keep HTTPS login redirects and reloads inside the application instead of launching the default browser.
- Preserve Google/YouTube authentication popup windows and their opener context.

## 0.1.0

- Initial Windows client using Rust, Tauri, and WebView2.
- Persistent browser profile and window geometry.
- Native playback and navigation menus.
- Close-to-tray playback and single-instance behavior.
- Windows installer and portable package with SHA-256 checksums.
- GitHub Actions checks, builds, and draft releases for version tags.
