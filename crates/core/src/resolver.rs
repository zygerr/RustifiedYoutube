use crate::{model::valid_video_id, session};
use anyhow::{Context, Result};
use std::{io::Write, process::Stdio, time::Duration};
use tokio::process::Command;

pub struct PreparedAudio {
    pub url: String,
    _file: tempfile::TempPath,
}

pub async fn prepare(id: &str, cookie: Option<&str>) -> Result<PreparedAudio> {
    anyhow::ensure!(valid_video_id(id), "Invalid YouTube video ID");
    let directory = session::app_dir()?.join("audio-buffer");
    std::fs::create_dir_all(&directory)?;
    let file = tempfile::Builder::new()
        .prefix("track-")
        .suffix(".m4a")
        .tempfile_in(directory)?
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
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .context("YouTube took too long to prepare this track")?
        .context("The playback helper is missing. Reinstall RustifiedYoutube")?;
    drop(cookie_file);
    anyhow::ensure!(
        output.status.success() && std::fs::metadata(&file)?.len() > 1024,
        "YouTube could not prepare this track. Try another song or reconnect your account."
    );
    let url = reqwest::Url::from_file_path(&file)
        .map_err(|_| anyhow::anyhow!("Invalid audio buffer path"))?
        .to_string();
    Ok(PreparedAudio { url, _file: file })
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

pub async fn resolve(id: &str, cookie: Option<&str>) -> Result<String> {
    anyhow::ensure!(valid_video_id(id), "Invalid YouTube video ID");
    let mut command = Command::new(session::adjacent_tool("yt-dlp.exe"));
    command.args([
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--socket-timeout",
        "20",
        "--get-url",
        "--format",
        "bestaudio[ext=m4a]/bestaudio",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-js-runtimes",
    ]);
    command.arg("--js-runtimes").arg(format!(
        "node:{}",
        session::adjacent_tool("node.exe").display()
    ));
    let cookie_file = if let Some(cookie) = cookie {
        let mut file = tempfile::NamedTempFile::new_in(session::app_dir()?)?;
        file.write_all(netscape_cookie(cookie).as_bytes())?;
        command.arg("--cookies").arg(file.path());
        Some(file)
    } else {
        None
    };
    command
        .arg("--")
        .arg(format!("https://music.youtube.com/watch?v={id}"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(65), command.output())
        .await
        .context("YouTube took too long to prepare this track")?
        .context("The playback helper is missing. Reinstall RustifiedYoutube")?;
    drop(cookie_file);
    anyhow::ensure!(
        output.status.success(),
        "YouTube could not prepare this track. Try another song or reconnect your account."
    );
    let text = String::from_utf8(output.stdout)?;
    let value = text
        .lines()
        .find(|line| line.starts_with("https://"))
        .context("No playable stream was returned")?;
    let url = reqwest::Url::parse(value)?;
    anyhow::ensure!(
        url.host_str()
            .is_some_and(|h| h.ends_with(".googlevideo.com")),
        "Unexpected stream host"
    );
    Ok(value.into())
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
