use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: String,
    pub cover: String,
    pub liked: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover: String,
}

pub fn format_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn valid_video_id(value: &str) -> bool {
    value.len() == 11
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_accepts_only_video_ids() {
        assert!(valid_video_id("dQw4w9WgXcQ"));
        for value in [
            "--exec=bad",
            "https://example.com",
            "../cookies",
            "",
            "x\n123456789",
        ] {
            assert!(!valid_video_id(value));
        }
    }
    #[test]
    fn playback_clock_formats_minutes_and_clamps_negative_values() {
        assert_eq!(format_time(125.9), "2:05");
        assert_eq!(format_time(-1.0), "0:00");
    }
}
