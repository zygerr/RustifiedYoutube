# Changelog

## 0.3.17

- Start a shuffled queue with the selected track and shuffle every other track after it, so tracks listed above the selection are no longer skipped.
- Reset library and history loading state, errors and pending retries after Google sign-out so the next connection loads the library again.
- Replace a re-fetched cover's cache entry instead of letting its stale entry evict the new artwork early.
- Remove the temporary encrypted session file when saving the session fails.

## 0.3.16

- Recover from rejected library requests without treating them as Google logout. Preserve loaded playlists and history and retry their requests after transient failures.
- Refresh cookies from the confirmed playback account on a timer, including while the interface is idle. Retry incomplete captures sooner and reopen the saved playback profile once when its initial account check fails.
- Recover once from a stalled playback page, release the sign-in panel if its return to Music stalls, and preserve the selected account and track position during retries.

## 0.3.15

- Keep confirmed playback identity during temporary missing configuration and require a sustained explicit signed-out signal; Google account handoffs and individual track restrictions no longer declare the entire account signed out.
- Reject cookie callbacks from an old playback document or account, preserve credentials during incomplete refreshes and API authorization failures, and support secure authentication-cookie rotation.
- Initialize the media bridge when Music is interactive rather than waiting for every page resource to finish loading.
- Refine native navigation, search, typography, loading placeholders and fixed card geometry; reduce the persistent player to 96 px with aligned transport, artwork and volume controls.
- Disable full debug symbols and incremental snapshots in local development builds to reduce build-storage growth.

## 0.3.14

- Add a separate Google sign-out action while keeping native library disconnect independent from browser sign-in.
- Confirm sign-out in the playback WebView before clearing the encrypted native session, and ignore stale session captures during the transition.
- Avoid rescanning the entire Music document for media elements on every DOM mutation.
- Use RustifiedYoutube consistently for the visible wordmark and Windows executable description.

## 0.3.13

- Upgrade an existing legacy installation in place so Start Menu shortcuts and Windows relaunches open the native Rust interface.
- Keep a native executable at the legacy filename, refresh the legacy Start Menu shortcut, and update installation metadata without changing account data or enabling automatic startup.

## 0.3.12

- Center search text and its placeholder vertically with the search icon.

## 0.3.11

- Supply the Windows volume mixer with a standalone application ICO in persistent app data, avoiding executable resource locators that some mixer surfaces fail to display.
- Verify that Windows can load both large and small icons from that exact file during the mixer smoke check; retain the executable resource as a fallback if the ICO cannot be written.

## 0.3.10

- Label this player's Windows audio sessions as RustifiedYoutube and use the application's embedded icon in the volume mixer.
- Identify sessions through the actual dedicated WebView2 browser process and its verified process tree, including browser creation-time checks to reject reused process IDs.
- Refresh labels on active output devices in a background worker and retain session controls so names and icons survive while playback is paused.
- Keep mixer volume, mute, and routing untouched; add a live check for the audio-session name, embedded icon resource, and unchanged volume/mute values.

## 0.3.9

- Add native Windows taskbar thumbnail controls for Previous, Play/Pause, and Next, routed through the existing player and queue.
- Synchronize the center icon and tooltips with playback, disable unavailable actions, and show the current song and artist in the thumbnail tooltip.
- Capture taskbar creation before the main window is created and restore the toolbar after Explorer recreates the taskbar button.
- Generate transparent native icons that remain legible on light and dark taskbar previews; release icons, hooks, and COM resources on exit.
- Add a live muted taskbar check covering native command dispatch, pause/resume while minimized, previous/next queue navigation, and disabled Next at the end of the queue.

## 0.3.8

- Recover ignored or stalled YouTube Music navigation automatically using the exact requested watch URL, with one bounded recovery attempt per track.
- Use direct watch navigation for subsequent selections after detecting a broken SPA router, retaining the native queue and Google account selection.
- Distinguish a stalled navigation from an explicit Google sign-in requirement and avoid using another track's stale response to diagnose the account.
- Complete Google login inside the playback WebView, preserving its persistent profile and restoring the selected track, position, pause state, volume, and mode.
- Allow the official Google/YouTube authentication handoffs and handle account popups in that same WebView. Keep IPC restricted to the exact Music origin.
- Add Retry playback and Google sign-in actions to errors, plus regression checks for failed navigation, stale responses, direct reloads, and trusted account URLs.

## 0.3.7

- Add three native appearance themes in Settings: Classic, Purple Haze, and OLED, with clickable palette previews.
- Apply themes to navigation, search, catalog cards and rows, playback controls, the seek rail, dialogs, and standard widgets.
- Use deep purple surfaces and lavender accents for Purple Haze; keep the main surfaces and player background pure black in OLED.
- Apply theme changes immediately and save the choice with existing preferences, preserving Classic for previous installations.
- Extend isolated UI snapshots with theme selection for visual checks at desktop and compact sizes.

## 0.3.6

- Verify the actual playback page's Google login independently of the native library's saved API cookies.
- Hold playback when a saved account is expected but the media page is signed out, and show a playback reconnection action instead of silently playing as a guest.
- Open interactive Google sign-in when the player is disconnected even if playlists still load. Follow the official sign-in link from the connection window.
- Reload the playback document after interactive login, preserving the selected track, position, pause state, volume, and Audio/Video mode. Keep metadata-only refreshes uninterrupted.
- Preserve the selected Google account index in encrypted session storage and apply it to both playback and metadata requests.
- Keep the requested song source if an official Audio/Video toggle points at a different radio item after reloading.
- Reject playback updates from a replaced document and verify actual browser authentication in account smoke checks.

## 0.3.5

- Select the largest artwork source returned by YouTube Music instead of the thumbnail nearest 256 px.
- Request sharper 512 px catalog artwork and separate artwork up to 1536 px for the expanded player; retain the existing thumbnail until the larger image arrives.
- Try YouTube HD video thumbnails with standard/original fallbacks, preserve Google artwork crop options, and avoid upscaling a small source.
- Bound decoded artwork texture caching to 64 MiB and limit image download/decode sizes.
- Adjust volume by scrolling over the expanded song artwork or the current-song thumbnail, using the saved 1–100% scroll step. Show the current percentage when hovering the large cover.
- Keep independent wheel accumulation for artwork and the volume slider, consume scroll only over the target image, and add live artwork-resolution and input regression checks.

## 0.3.4

- Restore the native account directly from the application's persistent WebView2 profile at startup and refresh its cookies every five minutes.
- Reuse the browser's actual User-Agent for authenticated metadata requests and migrate previous encrypted session files automatically.
- Refresh rejected authenticated requests from the current browser session and retry once, with concurrent refreshes coordinated together.
- Keep saved credentials and the account connection across transient network/server failures; distinguish a rejected request from a confirmed Google sign-out.
- Make Continue with Google reuse an existing account first and open the login helper only when interactive sign-in is required. Refreshing no longer reloads or resets music playback.
- Require a confirmed signed-in YouTube Music page before the login helper saves the session; report cancelled sign-in correctly.
- Persist explicit native disconnection so background synchronization cannot reconnect it, and cancel an open sign-in when disconnected.
- Write DPAPI-protected session updates atomically, add session migration/error/concurrency checks, and test account restoration across restarts and during playback.

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
