# Changelog

## 0.3.3

- Restyle the native interface around YouTube Music's black surfaces, white typography and playback controls, red seek progress, search field, mood chips, and content shelves.
- Rebuild Discover as a single scrolling page with Listen again, a responsive Quick picks section, and Discover more.
- Keep Discover recommendations separate from search results when navigating between pages.
- Share fixed 180 px square artwork cards across Discover and Library, with bounded title/subtitle areas and responsive library columns.
- Share 48 px artwork and 64 px song rows across search, playlists, the queue, Quick picks, and the expanded player's upcoming queue.
- Center-crop wide and portrait thumbnails without stretching; loading placeholders occupy the same space as loaded artwork.
- Add offline fixture snapshots for visual inspection at 1440 × 900 and 1080 × 700, plus layout/cropping and row-action regression checks.

## 0.3.2

- Add native Discord Rich Presence with the current song, artist, artwork, playback timestamps, and a YouTube Music link; reconnect automatically when Discord starts again.
- Add a saved Discord toggle and connection status in Settings. Clear presence on pause, advertisements, disabled sharing, or exit. No Discord account token is required.
- Keep the footer height, song information, controls, and seek rail in place when opening or closing the expanded player.
- Apply the saved volume before the media engine starts, cap source volume synchronously, and reapply the limit on media lifecycle changes.
- Preserve YouTube's lower loudness-normalized levels instead of periodically raising them to the raw slider level.
- Add media-volume regression checks and Discord payload, framing, handshake, and Unicode coverage.

## 0.3.1

- Stretch the seek rail across the compact player and scale the expanded rail to the media panel.
- Keep a clicked or dragged seek position visible until the media engine confirms that specific request; ignore stale clock updates.
- Add persistent Shuffle for upcoming tracks, preserve the current song and played queue, and restore the remaining order when disabled.
- Show a native Listen again row on Discover with account listening history and locally remembered plays, artwork, and one-click playback.
- Remember a track only after playback starts successfully and add regression checks for seeking, responsive width, shuffle order, and history metadata.

## 0.3.0

- Replace full-track fetching/remuxing with the persistent official YouTube Music streaming engine, hosted by WebView2 inside the Rust application.
- Keep search, library, settings, controls, and the queue in egui; show only the video surface when Video mode is selected.
- Open an expanded player by clicking the current song/artwork in the bottom-left corner, with Audio/Video switching and an upcoming queue on the right.
- Load Windows symbol, math, and emoji fallback fonts to render styled Unicode titles, including a supplied sample title.
- Retain configurable volume wheel steps, tray playback, version display, and native queue navigation.
- Remove yt-dlp, Node.js, FFmpeg, full audio buffers, and the former prefetch/cache pipeline from the application package.
- Add a real streaming check for startup latency, video, pause, seek, and Audio/Video switching.

## 0.2.2

- Reduce audio extraction requests, with fallback to the compatible extraction path when needed.
- Preload the next queued track and reuse up to three recent prepared tracks within a 160 MB temporary cache.
- Prioritize a selected track over unrelated prefetch work and reuse an in-progress preload when that track is selected.
- Add volume adjustment by mouse wheel while hovering the volume control, with the exact step configurable from 1% to 100% in Settings (default 5%).
- Show the current volume percentage and persist the scroll-step setting.

## 0.2.1

- Repair fragmented M4A containers before Windows playback, fixing tracks that immediately jumped to EOF and skipped.
- Reject premature end events, isolate events per track, and ignore stale player updates when changing songs.
- Show which audio preparation stage is running and report playback startup failures instead of waiting indefinitely.
- Search both songs and music videos, offer All/Songs/Videos filters, and prioritize matching artists.
- Preserve complete title and artist text, including highlighted fragments, and deduplicate results by video ID.
- Bundle the pinned FFmpeg helper used to repair audio containers without re-encoding.

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
