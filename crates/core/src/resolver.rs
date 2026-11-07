use crate::{model::valid_video_id, session};
use anyhow::{Context, Result};
use std::{io::Write, process::Stdio, sync::Arc, time::Duration};
use tokio::process::Command;

#[derive(Clone)]
pub struct PreparedAudio {
    pub url: String,
    pub bytes: u64,
    _file: Arc<tempfile::TempPath>,
}

pub async fn prepare(id: &str, cookie: Option<&str>) -> Result<PreparedAudio> {
    prepare_with_status(id, cookie, |_| {}).await
}

pub async fn prepare_with_status(
    id: &str,
    cookie: Option<&str>,
    status: impl Fn(&str),
) -> Result<PreparedAudio> {
    anyhow::ensure!(valid_video_id(id), "Invalid YouTube video ID");
    status("Fetching audio from YouTube…");
    let directory = session::app_dir()?.join("audio-buffer");
    std::fs::create_dir_all(&directory)?;
    let file = tempfile::Builder::new()
        .prefix("track-")
        .suffix(".m4a")
        .tempfile_in(&directory)?
        .into_temp_path();
    let mut command = Command::new(session::adjacent_tool("yt-dlp.exe"));
    command.args([
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-js-runtimes",
        "--no-part",
        "--no-mtime",
        "--force-overwrites",
        "--fixup",
        "never",
        "--socket-timeout",
        "20",
        "--max-filesize",
        "100M",
        "--format",
        "bestaudio[ext=m4a]",
    ]);
    command.arg("--js-runtimes").arg(format!(
        "node:{}",
        session::adjacent_tool("node.exe").display()
    ));
    command.arg("--output").arg(&file);
    let cookie_file = if let Some(cookie) = cookie {
        let mut file = tempfile::NamedTempFile::new_in(session::app_dir()?)?;
        file.write_all(netscape_cookie(cookie).as_bytes())?;
        command.arg("--cookies").arg(file.path());
        Some(file)
    } else {
        None
    };
    let fallback_args: Vec<_> = command
        .as_std()
        .get_args()
        .map(std::ffi::OsString::from)
        .collect();
    command
        .arg("--extractor-args")
        .arg("youtube:skip=hls,dash;player_skip=initial_data");
    command
        .arg("--")
        .arg(format!("https://music.youtube.com/watch?v={id}"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("YouTube took too long to prepare this track")?
        .context("The playback helper is missing. Reinstall RustifiedYoutube")?;
    if !output.status.success() {
        status("Trying the compatible audio source…");
        let mut fallback = Command::new(session::adjacent_tool("yt-dlp.exe"));
        fallback
            .args(fallback_args)
            .arg("--")
            .arg(format!("https://music.youtube.com/watch?v={id}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        fallback.creation_flags(0x08000000);
        output = tokio::time::timeout(Duration::from_secs(60), fallback.output())
            .await
            .context("YouTube took too long to prepare this track")??;
    }
    drop(cookie_file);
    anyhow::ensure!(
        output.status.success() && std::fs::metadata(&file)?.len() > 1024,
        "YouTube could not prepare this track. Try another song or reconnect your account."
    );
    // DASH M4A can jump directly to EOF in Windows Media Foundation. Flatten
    // its fragments into an ordinary MP4 container without re-encoding audio.
    status("Preparing audio for Windows…");
    let fixed = tempfile::Builder::new()
        .prefix("play-")
        .suffix(".m4a")
        .tempfile_in(directory)?
        .into_temp_path();
    let mut remux = Command::new(session::adjacent_tool("ffmpeg.exe"));
    remux
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&file)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(&fixed)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    remux.creation_flags(0x08000000);
    let result = tokio::time::timeout(Duration::from_secs(30), remux.output())
        .await
        .context("Preparing native audio took too long")?
        .context("The audio repair helper is missing. Reinstall RustifiedYoutube")?;
    anyhow::ensure!(
        result.status.success() && std::fs::metadata(&fixed)?.len() > 1024,
        "This track's audio could not be prepared for Windows playback. Try another result."
    );
    let url = reqwest::Url::from_file_path(&fixed)
        .map_err(|_| anyhow::anyhow!("Invalid audio buffer path"))?
        .to_string();
    let bytes = std::fs::metadata(&fixed)?.len();
    Ok(PreparedAudio {
        url,
        bytes,
        _file: Arc::new(fixed),
    })
}

fn netscape_cookie(cookie: &str) -> String {
    let mut text = "# Netscape HTTP Cookie File\n".to_string();
    for part in cookie.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && !name.is_empty()
            && !name.contains(['\n', '\r', '\t'])
            && !value.contains(['\n', '\r', '\t'])
        {
            text.push_str(&format!(
                ".youtube.com\tTRUE\t/\tTRUE\t0\t{name}\t{value}\n"
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookie_file_does_not_allow_line_injection() {
        let cookie = netscape_cookie("SAPISID=fake; bad\nname=bad; valid=ok");
        assert!(cookie.contains("SAPISID\tfake"));
        assert!(!cookie.contains("bad"));
        assert!(cookie.contains("valid\tok"));
    }
}
