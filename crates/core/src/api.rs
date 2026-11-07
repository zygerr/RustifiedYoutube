use crate::model::{Playlist, Track};
use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use ytmapi_rs::{
    YtMusic,
    auth::{BrowserToken, noauth::NoAuthToken},
    common::{LikeStatus, PlaylistID, Thumbnail, VideoID, YoutubeID},
    parse::{PlaylistItem, SearchResultSong},
    query::GetPlaylistTracksQuery,
};

pub struct Catalog {
    guest: YtMusic<NoAuthToken>,
    signed: Option<YtMusic<BrowserToken>>,
}

fn cover(images: &[Thumbnail]) -> String {
    images
        .iter()
        .min_by_key(|x| x.width.abs_diff(256))
        .map(|x| x.url.clone())
        .unwrap_or_default()
}

fn search_track(s: SearchResultSong) -> Track {
    Track {
        id: s.video_id.get_raw().into(),
        title: s.title,
        artist: s.artist,
        album: s.album.map(|a| a.name).unwrap_or_default(),
        duration: s.duration,
        cover: cover(&s.thumbnails),
        liked: false,
    }
}

fn playlist_track(item: PlaylistItem) -> Option<Track> {
    match item {
        PlaylistItem::Song(s) if s.is_available => Some(Track {
            id: s.video_id.get_raw().into(),
            title: s.title,
            artist: s
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            album: s.album.name,
            duration: s.duration,
            cover: cover(&s.thumbnails),
            liked: s.like_status == LikeStatus::Liked,
        }),
        PlaylistItem::Video(s) if s.is_available => Some(Track {
            id: s.video_id.get_raw().into(),
            title: s.title,
            artist: s.channel_name,
            duration: s.duration,
            cover: cover(&s.thumbnails),
            liked: s.like_status == LikeStatus::Liked,
            ..Default::default()
        }),
        _ => None,
    }
}

impl Catalog {
    pub async fn new(cookie: Option<&str>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .http1_only()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0",
            )
            .timeout(std::time::Duration::from_secs(25))
            .build()?;
        // The library bootstrap appends an empty query string to the homepage.
        // Fetch the canonical URL directly and deserialize its documented tokens.
        let request = http
            .get("https://music.youtube.com/")
            .header("Cookie", cookie.unwrap_or("SOCS=CAI"));
        let html = request.send().await?.error_for_status()?.text().await?;
        let visitor_id = config_string(&html, "VISITOR_DATA")?;
        let client_version = config_string(&html, "INNERTUBE_CLIENT_VERSION")?;
        let guest_token: NoAuthToken = serde_json::from_value(
            serde_json::json!({"create_time": chrono::Utc::now(), "visitor_id": visitor_id}),
        )?;
        let client = ytmapi_rs::Client::new_from_reqwest_client(http);
        let guest = ytmapi_rs::YtMusicBuilder::new_with_client(client.clone())
            .with_auth_token(guest_token)
            .build()?;
        let signed = match cookie {
            Some(cookie) => {
                let sapisid = cookie
                    .split(';')
                    .find_map(|part| part.trim().strip_prefix("SAPISID="))
                    .context("Google session is incomplete")?;
                let token: BrowserToken = serde_json::from_value(
                    serde_json::json!({"sapisid": sapisid, "client_version": client_version, "cookies": cookie}),
                )?;
                Some(
                    ytmapi_rs::YtMusicBuilder::new_with_client(client)
                        .with_auth_token(token)
                        .build()?,
                )
            }
            None => None,
        };
        Ok(Self { guest, signed })
    }

    pub fn signed_in(&self) -> bool {
        self.signed.is_some()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let songs = match &self.signed {
            Some(api) => api.search_songs(query).await?,
            None => self.guest.search_songs(query).await?,
        };
        Ok(songs.into_iter().map(search_track).collect())
    }

    pub async fn library(&self) -> Result<Vec<Playlist>> {
        let api = self
            .signed
            .as_ref()
            .context("Connect your Google account to open your library")?;
        let mut playlists = Vec::new();
        let mut continuation = None;
        for _ in 0..30 {
            let page = api
                .query(LibraryQuery {
                    continuation: continuation.clone(),
                })
                .await?;
            for playlist in page.playlists {
                if !playlists.iter().any(|p: &Playlist| p.id == playlist.id) {
                    playlists.push(playlist);
                }
            }
            if page.continuation.is_none() || page.continuation == continuation {
                break;
            }
            continuation = page.continuation;
        }
        Ok(playlists)
    }

    pub async fn playlist(&self, id: &str) -> Result<Vec<Track>> {
        let browse_id = if id.starts_with("VL") {
            id.to_string()
        } else {
            format!("VL{id}")
        };
        let query = GetPlaylistTracksQuery::new(PlaylistID::from_raw(browse_id));
        let pages: Vec<Vec<PlaylistItem>> = match &self.signed {
            Some(api) => api.stream(&query).take(100).try_collect().await?,
            None => self.guest.stream(&query).take(100).try_collect().await?,
        };
        Ok(pages
            .into_iter()
            .flatten()
            .filter_map(playlist_track)
            .collect())
    }

    pub async fn like(&self, id: &str, liked: bool) -> Result<()> {
        let api = self
            .signed
            .as_ref()
            .context("Connect your account to like songs")?;
        api.rate_song(
            VideoID::from_raw(id),
            if liked {
                LikeStatus::Liked
            } else {
                LikeStatus::Indifferent
            },
        )
        .await?;
        Ok(())
    }
}

struct LibraryQuery {
    continuation: Option<String>,
}
#[derive(Debug)]
struct LibraryPage {
    playlists: Vec<Playlist>,
    continuation: Option<String>,
}

impl ytmapi_rs::query::Query<BrowserToken> for LibraryQuery {
    type Output = LibraryPage;
    type Method = ytmapi_rs::query::PostMethod;
}
impl ytmapi_rs::query::PostQuery for LibraryQuery {
    fn header(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::from_iter([("browseId".into(), "FEmusic_liked_playlists".into())])
    }
    fn path(&self) -> &str {
        "browse"
    }
    fn params(&self) -> Vec<(&str, std::borrow::Cow<'_, str>)> {
        match &self.continuation {
            Some(value) => vec![
                ("ctoken", value.as_str().into()),
                ("continuation", value.as_str().into()),
            ],
            None => vec![],
        }
    }
}
impl ytmapi_rs::parse::ParseFrom<LibraryQuery> for LibraryPage {
    fn parse_from(result: ytmapi_rs::ProcessedResult<LibraryQuery>) -> ytmapi_rs::Result<Self> {
        let json: serde_json::Value = ytmapi_rs::json::from_json(result.json)?;
        let mut page = Self {
            playlists: Vec::new(),
            continuation: None,
        };
        collect_library(&json, &mut page);
        Ok(page)
    }
}

fn collect_library(value: &serde_json::Value, page: &mut LibraryPage) {
    if let Some(card) = value.get("musicTwoRowItemRenderer") {
        let id = card
            .pointer("/navigationEndpoint/browseEndpoint/browseId")
            .and_then(|v| v.as_str());
        let title = card.pointer("/title/runs/0/text").and_then(|v| v.as_str());
        if let (Some(id), Some(title)) = (id, title)
            && let Some(id) = id.strip_prefix("VL")
        {
            let subtitle = card
                .pointer("/subtitle/runs")
                .and_then(|v| v.as_array())
                .map(|runs| {
                    runs.iter()
                        .filter_map(|run| run.get("text").and_then(|v| v.as_str()))
                        .collect::<String>()
                })
                .unwrap_or_default();
            let images: Vec<Thumbnail> = card
                .pointer("/thumbnailRenderer/musicThumbnailRenderer/thumbnail/thumbnails")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            page.playlists.push(Playlist {
                id: id.into(),
                title: title.into(),
                subtitle,
                cover: cover(&images),
            });
        }
        return;
    }
    if let Some(token) = value
        .pointer("/nextContinuationData/continuation")
        .and_then(|v| v.as_str())
    {
        page.continuation = Some(token.into());
    }
    match value {
        serde_json::Value::Object(fields) => {
            for child in fields.values() {
                collect_library(child, page);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_library(child, page);
            }
        }
        _ => {}
    }
}

fn config_string(html: &str, key: &str) -> Result<String> {
    let marker = format!("\"{key}\":");
    let (_, value) = html
        .split_once(&marker)
        .context("YouTube Music did not return its configuration. Try connecting your account.")?;
    let mut decoder = serde_json::Deserializer::from_str(value.trim_start());
    Ok(serde::Deserialize::deserialize(&mut decoder)?)
}

#[cfg(test)]
mod bootstrap_tests {
    use super::config_string;
    #[test]
    fn configuration_reads_one_json_value_without_truncating_escaped_strings() {
        assert_eq!(
            config_string(
                r#"ytcfg.set({"VISITOR_DATA":"abc\"def","OTHER":1});"#,
                "VISITOR_DATA"
            )
            .unwrap(),
            "abc\"def"
        );
        assert!(config_string("<html>No config</html>", "VISITOR_DATA").is_err());
    }

    #[test]
    fn library_tolerates_short_subtitles_and_keeps_continuations() {
        let data = serde_json::json!({"items":[
            {"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Create playlist"}]}}},
            {"musicTwoRowItemRenderer":{
                "navigationEndpoint":{"browseEndpoint":{"browseId":"VLPLtest"}},
                "title":{"runs":[{"text":"My playlist"}]},
                "subtitle":{"runs":[{"text":"1 song"}]}
            }}], "continuations":[{"nextContinuationData":{"continuation":"next-page"}}]});
        let mut page = super::LibraryPage {
            playlists: Vec::new(),
            continuation: None,
        };
        super::collect_library(&data, &mut page);
        assert_eq!(page.playlists.len(), 1);
        assert_eq!(page.playlists[0].id, "PLtest");
        assert_eq!(page.playlists[0].subtitle, "1 song");
        assert_eq!(page.continuation.as_deref(), Some("next-page"));
    }
}
