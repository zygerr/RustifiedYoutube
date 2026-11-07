# Bundled components

RustifiedYoutube uses eframe/egui and wgpu for its native interface, Windows MediaPlayer for audio, and ytmapi-rs for YouTube Music metadata. `RUST-DEPENDENCIES.txt` in packaged builds lists Cargo dependencies, their license expressions, and source repositories. Their applicable open source licenses remain in effect.

The standalone yt-dlp Windows executable is distributed under its upstream licenses, including GPL-3.0 components. Its full notices are bundled as `YT-DLP-LICENSE.txt` and `YT-DLP-THIRD-PARTY-LICENSES.txt`. Corresponding upstream source is available at https://github.com/yt-dlp/yt-dlp/tree/2026.10.26 and its release page https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26. It runs as a separate process.

The official Node.js 24.18.0 runtime is bundled solely for the playback helper's JavaScript challenges. Its licenses are in `NODE-LICENSE.txt`; source is available at https://nodejs.org/dist/v24.18.0/ and https://github.com/nodejs/node/tree/v24.18.0.

WebView2 is a Microsoft system runtime used only by the temporary Google sign-in helper. The main interface and audio playback do not use it.

This project is independent and is not affiliated with Google, YouTube, Microsoft, or the authors of these components.
