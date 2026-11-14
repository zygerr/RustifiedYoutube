use crate::model::{Playlist, SearchFilter, Track, valid_video_id};
use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use ytmapi_rs::{
    YtMusic,
    auth::{BrowserToken, noauth::NoAuthToken},
    common::{LikeStatus, PlaylistID, Thumbnail, VideoID, YoutubeID},
    parse::PlaylistItem,
    query::GetPlaylistTracksQuery,
};

pub struct Catalog {
    guest: YtMusic<NoAuthToken>,
    signed: Option<YtMusic<BrowserToken>>,
}

#[derive(Debug)]
pub struct SessionExpired;
impl std::fmt::Display for SessionExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Google sign-in needs to be renewed")
    }
}
impl std::error::Error for SessionExpired {}
#[derive(Debug)]
pub struct AuthRejected;
impl std::fmt::Display for AuthRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("YouTube did not authorize this request")
    }
}
impl std::error::Error for AuthRejected {}
pub fn is_auth_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|error| error.is::<SessionExpired>() || error.is::<AuthRejected>())
}
pub fn is_expired_error(error: &anyhow::Error) -> bool {
    error.chain().any(|error| error.is::<SessionExpired>())
}
fn api_error(error: ytmapi_rs::Error) -> anyhow::Error {
    match error.into_kind() {
        ytmapi_rs::error::ErrorKind::OtherErrorCodeInResponse {
            code: 401 | 403, ..
        } => AuthRejected.into(),
        // A malformed/unsupported request header does not prove that the
        // browser itself signed out. Refresh the API client without dropping
        // the saved account or forcing the playback engine into guest mode.
        ytmapi_rs::error::ErrorKind::Header => AuthRejected.into(),
        other => ytmapi_rs::Error::from(other).into(),
    }
}

fn cover(images: &[Thumbnail]) -> String {
    images
        .iter()
        .max_by_key(|x| x.width.saturating_mul(x.height))
        .map(|x| x.url.clone())
        .unwrap_or_default()
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
            is_video: false,
        }),
        PlaylistItem::Video(s) if s.is_available => Some(Track {
            id: s.video_id.get_raw().into(),
            title: s.title,
            artist: s.channel_name,
            duration: s.duration,
            cover: cover(&s.thumbnails),
            liked: s.like_status == LikeStatus::Liked,
            is_video: true,
            ..Default::default()
        }),
        _ => None,
    }
}

impl Catalog {
    pub async fn new(cookie: Option<&str>) -> Result<Self> {
        Self::bootstrap(cookie, None, None).await
    }
    pub async fn from_session(session: &crate::session::SavedSession) -> Result<Self> {
        Self::bootstrap(
            Some(&session.cookies),
            session.user_agent.as_deref(),
            session.auth_user,
        )
        .await
    }
    async fn bootstrap(
        cookie: Option<&str>,
        user_agent: Option<&str>,
        auth_user: Option<u8>,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(index) = auth_user {
            headers.insert("X-Goog-AuthUser", index.to_string().parse()?);
        }
        let http = reqwest::Client::builder()
            .http1_only()
            .default_headers(headers)
            .user_agent(user_agent.unwrap_or(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0",
            ))
            .timeout(std::time::Duration::from_secs(25))
            .build()?;
        // The library bootstrap appends an empty query string to the homepage.
        // Fetch the canonical URL directly and deserialize its documented tokens.
        let request = http
            .get(crate::session::music_url(auth_user))
            .header("Cookie", cookie.unwrap_or("SOCS=CAI"));
        let response = request.send().await?;
        if cookie.is_some() && matches!(response.status().as_u16(), 401 | 403) {
            return Err(AuthRejected.into());
        }
        let html = response.error_for_status()?.text().await?;
        if cookie.is_some() {
            check_authenticated_bootstrap(&html)?;
        }
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
                let sapisid = crate::session::authentication_cookie(cookie).ok_or(AuthRejected)?;
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
        self.search_filtered(query, SearchFilter::All).await
    }

    pub async fn search_filtered(&self, query: &str, filter: SearchFilter) -> Result<Vec<Track>> {
        let mut tracks = match filter {
            SearchFilter::Songs => self.search_page(query, false).await?,
            SearchFilter::Videos => self.search_page(query, true).await?,
            SearchFilter::All => {
                let (songs, videos) = tokio::join!(
                    self.search_page(query, false),
                    self.search_page(query, true)
                );
                match (songs, videos) {
                    (Ok(mut songs), Ok(videos)) => {
                        songs.extend(videos);
                        songs
                    }
                    (Ok(tracks), Err(_)) | (Err(_), Ok(tracks)) => tracks,
                    (Err(error), Err(_)) => return Err(error),
                }
            }
        };
        let mut seen = std::collections::HashSet::new();
        tracks.retain(|track| seen.insert(track.id.clone()));
        let query = normalized(query);
        tracks.sort_by_cached_key(|track| std::cmp::Reverse(relevance(track, &query)));
        Ok(tracks)
    }

    async fn search_page(&self, text: &str, video: bool) -> Result<Vec<Track>> {
        use ytmapi_rs::query::search::{
            FilteredSearch, SearchQuery, SongsFilter, SpellingMode, VideosFilter,
        };
        let json = if video {
            let query = SearchQuery::<FilteredSearch<VideosFilter>>::from(text)
                .with_spelling_mode(SpellingMode::WithSuggestions);
            match &self.signed {
                Some(api) => api.json_query(query).await.map_err(api_error)?,
                None => self.guest.json_query(query).await.map_err(api_error)?,
            }
        } else {
            let query = SearchQuery::<FilteredSearch<SongsFilter>>::from(text)
                .with_spelling_mode(SpellingMode::WithSuggestions);
            match &self.signed {
                Some(api) => api.json_query(query).await.map_err(api_error)?,
                None => self.guest.json_query(query).await.map_err(api_error)?,
            }
        };
        let json: serde_json::Value = ytmapi_rs::json::from_json(json)?;
        let mut tracks = Vec::new();
        collect_search(&json, video, &mut tracks);
        Ok(tracks)
    }

    pub async fn library(&self) -> Result<Vec<Playlist>> {
        let api = self.signed.as_ref().ok_or(SessionExpired)?;
        let mut playlists = Vec::new();
        let mut continuation = None;
        for _ in 0..30 {
            let page = api
                .query(LibraryQuery {
                    continuation: continuation.clone(),
                })
                .await
                .map_err(api_error)?;
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

    pub async fn history(&self) -> Result<Vec<Track>> {
        let api = self.signed.as_ref().ok_or(SessionExpired)?;
        let json: serde_json::Value = ytmapi_rs::json::from_json(
            api.json_query(ytmapi_rs::query::GetHistoryQuery)
                .await
                .map_err(api_error)?,
        )?;
        let mut tracks = Vec::new();
        collect_history(&json, &mut tracks);
        let mut seen = std::collections::HashSet::new();
        tracks.retain(|track| seen.insert(track.id.clone()));
        tracks.truncate(30);
        Ok(tracks)
    }

    pub async fn playlist(&self, id: &str) -> Result<Vec<Track>> {
        let browse_id = if id.starts_with("VL") {
            id.to_string()
        } else {
            format!("VL{id}")
        };
        let query = GetPlaylistTracksQuery::new(PlaylistID::from_raw(browse_id));
        let pages: Vec<Vec<PlaylistItem>> = match &self.signed {
            Some(api) => api
                .stream(&query)
                .take(100)
                .try_collect()
                .await
                .map_err(api_error)?,
            None => self
                .guest
                .stream(&query)
                .take(100)
                .try_collect()
                .await
                .map_err(api_error)?,
        };
        Ok(pages
            .into_iter()
            .flatten()
            .filter_map(playlist_track)
            .collect())
    }

    pub async fn like(&self, id: &str, liked: bool) -> Result<()> {
        let api = self.signed.as_ref().ok_or(SessionExpired)?;
        api.rate_song(
            VideoID::from_raw(id),
            if liked {
                LikeStatus::Liked
            } else {
                LikeStatus::Indifferent
            },
        )
        .await
        .map_err(api_error)?;
        Ok(())
    }
}

fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'ă' | 'â' => 'a',
            'î' => 'i',
            'ș' | 'ş' => 's',
            'ț' | 'ţ' => 't',
            _ => c,
        })
        .collect()
}

fn relevance(track: &Track, query: &str) -> usize {
    let artist = normalized(&track.artist);
    let title = normalized(&track.title);
    let tokens: Vec<_> = query.split_whitespace().collect();
    let all_artist = !tokens.is_empty() && tokens.iter().all(|token| artist.contains(token));
    tokens
        .iter()
        .filter(|token| title.contains(**token) || artist.contains(**token))
        .count()
        * 10
        + usize::from(all_artist) * 5
        + usize::from(artist == query) * 5
}

fn runs_text(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(|v| v.as_array())
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r.get("text").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

fn collect_search(value: &serde_json::Value, video: bool, tracks: &mut Vec<Track>) {
    if let Some(card) = value.get("musicResponsiveListItemRenderer") {
        if card
            .get("musicItemRendererDisplayPolicy")
            .and_then(|v| v.as_str())
            == Some("MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT")
        {
            return;
        }
        let id = card
            .pointer("/playlistItemData/videoId")
            .or_else(|| card.pointer("/navigationEndpoint/watchEndpoint/videoId"))
            .or_else(||card.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId"))
            .and_then(|v| v.as_str());
        let Some(id) = id.filter(|id| valid_video_id(id)) else {
            return;
        };
        let title = runs_text(
            card.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs"),
        );
        if title.trim().is_empty() {
            return;
        }
        let metadata = runs_text(
            card.pointer("/flexColumns/1/musicResponsiveListItemFlexColumnRenderer/text/runs"),
        );
        let fields: Vec<_> = metadata.split(" • ").collect();
        let artist_index =
            usize::from(video && matches!(fields.first().copied(), Some("Video" | "Song")));
        let artist = fields
            .get(artist_index)
            .copied()
            .unwrap_or_default()
            .to_string();
        let duration = fields
            .iter()
            .rev()
            .find(|field| {
                let parts: Vec<_> = field.split(':').collect();
                (2..=3).contains(&parts.len())
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
            })
            .copied()
            .unwrap_or_default()
            .to_string();
        let album = if !video && fields.len() >= 3 {
            fields[1].to_string()
        } else {
            String::new()
        };
        let images: Vec<Thumbnail> = card
            .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        tracks.push(Track {
            id: id.into(),
            title,
            artist,
            album,
            duration,
            cover: cover(&images),
            liked: false,
            is_video: video,
        });
        return;
    }
    match value {
        serde_json::Value::Object(fields) => {
            for child in fields.values() {
                collect_search(child, video, tracks);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_search(child, video, tracks);
            }
        }
        _ => {}
    }
}

fn collect_history(value: &serde_json::Value, tracks: &mut Vec<Track>) {
    if let Some(card) = value.get("musicResponsiveListItemRenderer") {
        let start = tracks.len();
        collect_search(value, false, tracks);
        if let Some(track) = tracks.get_mut(start) {
            let duration =
                runs_text(card.pointer(
                    "/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text/runs",
                ));
            if !duration.is_empty() {
                track.duration = duration;
            }
            let kind=card.pointer("/navigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
                .or_else(||card.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType"))
                .and_then(|value|value.as_str()).unwrap_or_default();
            track.is_video = matches!(kind, "MUSIC_VIDEO_TYPE_OMV" | "MUSIC_VIDEO_TYPE_UGC");
        }
        return;
    }
    match value {
        serde_json::Value::Object(fields) => {
            for child in fields.values() {
                collect_history(child, tracks);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_history(child, tracks);
            }
        }
        _ => {}
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
    config_value(html, key)
}
fn check_authenticated_bootstrap(html: &str) -> Result<()> {
    // A standalone HTTP request has a different context from the logged-in
    // WebView. A guest page or incomplete response cannot establish logout.
    if config_value::<bool>(html, "LOGGED_IN").ok() == Some(false) {
        return Err(AuthRejected.into());
    }
    Ok(())
}

fn config_value<T: serde::de::DeserializeOwned>(html: &str, key: &str) -> Result<T> {
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
    fn artwork_selection_uses_the_largest_available_source() {
        let thumbnails: Vec<ytmapi_rs::common::Thumbnail> =
            serde_json::from_value(serde_json::json!([
                {"url":"small","width":60,"height":60},
                {"url":"medium","width":256,"height":256},
                {"url":"large","width":1200,"height":1200}
            ]))
            .unwrap();
        assert_eq!(super::cover(&thumbnails), "large");
    }
    #[test]
    fn auth_rejections_refresh_but_do_not_prove_google_signed_out() {
        let header_error =
            super::api_error(ytmapi_rs::Error::from(ytmapi_rs::error::ErrorKind::Header));
        assert!(super::is_auth_error(&header_error));
        assert!(!super::is_expired_error(&header_error));
        for code in [401, 403] {
            let error =
                ytmapi_rs::Error::from(ytmapi_rs::error::ErrorKind::OtherErrorCodeInResponse {
                    code,
                    message: "fixture".into(),
                });
            let error = super::api_error(error);
            assert!(super::is_auth_error(&error));
            assert!(!super::is_expired_error(&error));
        }
        for code in [429, 500, 503] {
            let error =
                ytmapi_rs::Error::from(ytmapi_rs::error::ErrorKind::OtherErrorCodeInResponse {
                    code,
                    message: "fixture".into(),
                });
            assert!(!super::is_auth_error(&super::api_error(error)));
        }
        assert!(!super::is_auth_error(&anyhow::anyhow!("Network timeout")));
        assert!(
            !super::config_value::<bool>(
                r#"ytcfg.set({"LOGGED_IN":false,"OTHER":true});"#,
                "LOGGED_IN"
            )
            .unwrap()
        );
        assert!(
            super::config_value::<bool>(r#"ytcfg.set({"LOGGED_IN":true});"#, "LOGGED_IN").unwrap()
        );
    }
    #[test]
    fn guest_library_bootstrap_requests_recovery_without_reporting_expiry() {
        let error =
            super::check_authenticated_bootstrap(r#"ytcfg.set({"LOGGED_IN":false});"#).unwrap_err();
        assert!(super::is_auth_error(&error));
        assert!(!super::is_expired_error(&error));
        super::check_authenticated_bootstrap(r#"ytcfg.set({"LOGGED_IN":true});"#).unwrap();
    }

    #[test]
    fn history_keeps_order_full_titles_and_fixed_duration() {
        let row = |id, title: &str| {
            serde_json::json!({"musicResponsiveListItemRenderer": {
                "flexColumns":[
                    {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[
                        {"text":title,"navigationEndpoint":{"watchEndpoint":{"videoId":id,"watchEndpointMusicSupportedConfigs":{"watchEndpointMusicConfig":{"musicVideoType":"MUSIC_VIDEO_TYPE_OMV"}}}}},
                        {"text":" 🚀"}
                    ]}}},
                    {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Nova Lane"},{"text":" • Album"}]}}}
                ],
                "fixedColumns":[{"musicResponsiveListItemFixedColumnRenderer":{"text":{"runs":[{"text":"5:00"}]}}}]
            }})
        };
        let data = serde_json::json!({"contents":[row("Fixture0001", "𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬"),row("Fixture0002", "Next song")]});
        let mut tracks = Vec::new();
        super::collect_history(&data, &mut tracks);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, "Fixture0001");
        assert_eq!(tracks[1].id, "Fixture0002");
        assert_eq!(tracks[0].title, "𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬 🚀");
        assert_eq!(tracks[0].artist, "Nova Lane");
        assert_eq!(tracks[0].duration, "5:00");
        assert!(tracks[0].is_video);
    }
    #[test]
    fn search_preserves_highlighted_title_fragments_and_byline() {
        let data = serde_json::json!({"musicResponsiveListItemRenderer": {
            "playlistItemData":{"videoId":"Fixture0001"},
            "flexColumns":[
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"ECHO AVENUE - "},{"text":"NIGHT"},{"text":" SHIFT"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Nova "},{"text":"Lane"},{"text":" • "},{"text":"100K views"},{"text":" • "},{"text":"3:28"}]}}}
            ]
        }});
        let mut tracks = Vec::new();
        super::collect_search(&data, true, &mut tracks);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "ECHO AVENUE - NIGHT SHIFT");
        assert_eq!(tracks[0].artist, "Nova Lane");
        assert_eq!(tracks[0].duration, "3:28");
        assert!(tracks[0].is_video);
    }
    #[test]
    fn artist_matches_rank_ahead_of_other_channels() {
        let official = crate::model::Track {
            title: "NIGHT SHIFT".into(),
            artist: "Nova Lane".into(),
            ..Default::default()
        };
        let other = crate::model::Track {
            artist: "Another channel".into(),
            ..official.clone()
        };
        assert!(
            super::relevance(&official, "nova lane")
                > super::relevance(&other, "nova lane")
        );
        assert_eq!(super::normalized("LUPTĂ ȘI ȚARĂ"), "lupta si tara");
    }
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
