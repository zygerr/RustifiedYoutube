# Development

## Requirements

Windows 10/11 x64, a DirectX 12 compatible graphics driver, Rust stable with the MSVC toolchain, Microsoft C++ Build Tools, and the [Microsoft WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/). Node.js 24 is only needed to run the media bridge tests; it is not bundled or used for playback.

## Project layout

| Path | Contents |
| --- | --- |
| `native/` | The Windows app: egui interface, WebView2 player, tray, taskbar, mixer, Discord, logging |
| `native/src/media_bridge.js` | Script injected into the YouTube Music page that drives the official player |
| `crates/core/` | YouTube Music catalog API, data model and encrypted session storage |
| `auth-helper/` | Legacy standalone sign-in recovery tool (`rustified-auth.exe`) |
| `installer/`, `scripts/` | NSIS installer and build/packaging scripts |

## How it works

The interface is Rust (egui on wgpu/DirectX 12). Playback runs the official YouTube Music player in a persistent WebView2 surface; in Video mode the WebView shows only the media surface. Nothing is downloaded or remuxed, and yt-dlp, Node.js and FFmpeg are not used. The playback engine warms up at launch and stays alive between tracks. Artwork loads at up to 512 px for browsing and 1536 px in the expanded player (wide thumbnails are center-cropped), with a 64 MiB decoded-image cache. Search, library and history use the YouTube Music API directly.

Only official HTTPS Music and Google/YouTube account handoffs may load in the WebView; approved account popups continue in the same WebView, and IPC is accepted only from the exact Music origin.

If YouTube ignores a track selection, the player retries once using that exact watch URL. After detecting a broken SPA router, later selections use direct watch navigation for the rest of the session. Recovery preserves the native queue and account selection; a navigation failure alone is never treated as a Google logout.

## Accounts and local data

Google sign-in opens inside the same WebView used for playback, backed by the app's persistent profile at `%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\webview`. Returning from Google restores the selected track, position, pause state, volume and Audio/Video mode.

The library API session is synchronized from that browser profile at startup and every five minutes and stored as `session.dpapi`, encrypted with Windows DPAPI for the current user and written atomically. Cookies are read through native WebView2 APIs (including HttpOnly cookies) and never pass through JavaScript IPC, logs or Discord. Rejected authenticated requests trigger one coordinated refresh and retry. Temporary connection or server errors, unavailable page configuration, restricted tracks and incomplete cookie refreshes never count as sign-out, and secure authentication-cookie rotation is accepted.

A saved library credential does not prove playback is authenticated: with an account saved, playback waits for the media page to confirm sign-in, and a signed-out player offers reconnection instead of silently playing as a guest.

- **Disconnect native session** removes `session.dpapi` and writes `session.disabled` to stop automatic reconnection. Google stays signed in.
- **Sign out of Google in this app** runs Google's logout in the app's profile, confirms the player is signed out, then removes the native session.
- Uninstalling keeps user data. UI preferences live in eframe's per-user directory (`%APPDATA%\RustifiedYoutube\data`).

### Debug log

`%LOCALAPPDATA%\io.github.zygerr.rustifiedyoutube\logs\RustifiedYoutube.log` records play requests, meaningful player state changes, stalled-page reports and recoveries, shown errors, account and Discord status, blocked navigation hosts (host only) and crashes. When a track does not start within 40 seconds or the page does not load, it also records a snapshot of the page's player status. It rotates at 4 MiB, keeping `RustifiedYoutube.old.log`. Lines that carry credential values are redacted.

## Windows integration

- **Volume mixer** — audio sessions are labeled *RustifiedYoutube* with the app icon (a standalone ICO in app data, with an executable-resource fallback). Volume, mute and routing are left untouched.
- **Taskbar** — thumbnail Previous / Play-Pause / Next buttons stay in sync, work while minimized and disable Next at the end of the queue unless Repeat is on.
- **Media keys** — the page's Media Session actions are claimed before YouTube's scripts run and routed to the app's queue, so headset buttons and the Windows media flyout behave the same.
- **Discord Rich Presence** — uses Discord's [local RPC protocol](https://docs.discord.com/developers/topics/rpc) on a background thread with automatic reconnection. The default public Application ID `1568660934296076398` can be replaced under **Settings → Discord Rich Presence → Application settings**. No login, bot token or client secret is needed, and Google session data never enters the payload.

## Checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test native/tests/media_bridge.cjs
```

### Live checks

These contact YouTube and use the app's real profile:

```powershell
cargo run -p rustified-core --example smoke -- --authenticated
./target/debug/RustifiedYoutube.exe --smoke-stream                 # two public streams, muted
./target/debug/RustifiedYoutube.exe --smoke-stream --smoke-taskbar
./target/debug/RustifiedYoutube.exe --smoke-stream --smoke-mixer
./target/debug/RustifiedYoutube.exe --smoke-stream --smoke-recovery --smoke-login-rebind
./target/debug/RustifiedYoutube.exe --smoke-rpc                    # Discord handshake only
./target/debug/RustifiedYoutube.exe --smoke-account
./target/debug/RustifiedYoutube.exe --smoke-player-session
./target/debug/RustifiedYoutube.exe --smoke-login                  # may need you to sign in
./target/debug/RustifiedYoutube.exe --smoke-sign-out               # signs the app's profile out
./target/debug/RustifiedYoutube.exe --smoke-artwork
```

- **Stream** — opens a temporary instance at volume zero, measures two public streams (override with `--smoke-video <id> --smoke-video-2 <id> --smoke-title <word>`; the first must be a music video that also exists as a song), and verifies video dimensions, pause, seek confirmation and Audio/Video switching. It also refreshes the account during playback and checks the track and position survive. Saved UI preferences are not changed.
- **Taskbar** — minimizes the muted test window and sends real thumbnail-button clicks, checking pause/resume, Previous/Next, icon states and the disabled Next at the end of the queue.
- **Mixer** — verifies the audio-session name and icon path and that Windows can load the icon, without changing volume or mute. It does not check the mixer UI visually.
- **Recovery / rebind** — ignores the first SPA selection on purpose, verifies direct-watch recovery, then paused-position restoration through Google sign-in and queue advancement.
- **RPC** — verifies the Discord handshake and a clear-activity acknowledgement without showing a test song.
- **Account / player session** — require the playback document itself to be signed in, read library and history counts without logging names, refresh the connection and exit. They never open an interactive login, play music or publish Discord presence.
- **Login** — opens Google sign-in inside the playback WebView and verifies the return to an authenticated player and library.
- **Sign-out** — intentionally signs the app's Google profile out and verifies the native session is cleared.
- **Artwork** — fetches public images and verifies expanded artwork keeps more resolution than browsing artwork.

### UI snapshots

Debug builds can render pages offline with fixture data — no account, media or Discord access:

```powershell
./target/debug/RustifiedYoutube.exe --ui-snapshot discover
./target/debug/RustifiedYoutube.exe --ui-snapshot settings --theme purple --font SegoeUi --compact
```

Pages: `discover`, `library`, `search`, `queue`, `player`, `settings`. Options: `--theme purple|oled`, `--font <Name>` (for example `SegoeUi`, `Georgia`), and `--compact` for 1080 × 700 instead of 1440 × 900. PNGs are saved to `target/ui-preview/` and the app closes after capture. The README screenshots in `docs/screenshots/` come from this mode.

## Building and releasing

```powershell
./scripts/build-native.ps1
```

This builds the release binaries and writes the per-user installer, portable ZIP and SHA-256 checksums to `dist/<version>/`. The packaging script downloads only a pinned, checksum-verified NSIS build tool. Builds are not code-signed. The installer upgrades an existing installation in place (including the legacy `%LOCALAPPDATA%\RustifiedYoutube` layout) and keeps account data and preferences.

The [Windows workflow](https://github.com/zygerr/RustifiedYoutube/actions/workflows/windows.yml) checks and builds every push to `main` and every pull request. To publish a release:

1. Bump `version` in the root `Cargo.toml`, run `cargo update --workspace`, and add a `CHANGELOG.md` entry.
2. Commit, then push a matching tag: `git tag -a v0.3.24 -m "RustifiedYoutube 0.3.24"` and `git push origin v0.3.24`.
3. The workflow builds the installer, portable ZIP and checksums and creates a **draft** [release](https://github.com/zygerr/RustifiedYoutube/releases). Review it and publish.

## Credits

The streaming approach follows [ytmdesktop's integration with the official player](https://github.com/ytmdesktop/ytmdesktop/blob/development/src/renderer/ytmview/preload.ts), and the in-app login follows the persistent browser-session pattern in [its YTMViewManager](https://github.com/ytmdesktop/ytmdesktop/blob/d489670733dbd4af83420e802a069f64481aebc5/src/main/services/ytmviewmanager/index.ts). RustifiedYoutube has its own Rust interface and media adapter.
