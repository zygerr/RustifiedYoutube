# Third-party components

RustifiedYoutube uses eframe/egui and wgpu for its native Rust interface, wry for the WebView2 media surface and sign-in window, and ytmapi-rs for metadata. `RUST-DEPENDENCIES.txt` in packaged builds lists Cargo dependencies, their license expressions, and source repositories. Their applicable open source licenses remain in effect.

Microsoft WebView2 is a separately installed Microsoft runtime. It renders the official YouTube Music media player and the sign-in page. YouTube's pages and media are loaded from their own services; they are not redistributed in this package.

Windows font files are loaded from the local Windows Fonts directory for Unicode coverage. They remain system resources and are not bundled.

Version 0.3 no longer bundles yt-dlp, Node.js, or FFmpeg. The streaming architecture was researched using https://github.com/ytmdesktop/ytmdesktop. RustifiedYoutube uses its own media bridge rather than bundling that application.

This project is independent and is not affiliated with Google, YouTube, Microsoft, or the authors of its dependencies.
