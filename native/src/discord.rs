use crate::player::PlayerState;
use rustified_core::model::{Track, valid_video_id};
use serde_json::{Value, json};
use std::{
    io,
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    sync::watch,
    time::{Instant, sleep, timeout},
};

pub const APPLICATION_ID: &str = "1568660934296076398";
const MAX_FRAME: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// What Discord shows after "Listening to" in your status and the member list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatusDisplay {
    /// The Discord application's name ("RustifiedYoutube").
    AppName,
    /// The artist of the current song.
    #[default]
    Artist,
    /// The title of the current song.
    Song,
}
impl StatusDisplay {
    pub const ALL: [Self; 3] = [Self::Artist, Self::Song, Self::AppName];
    pub fn label(self) -> &'static str {
        match self {
            Self::AppName => "App name",
            Self::Artist => "Artist",
            Self::Song => "Song",
        }
    }
    /// Discord's `status_display_type`: 0 name, 1 state (artist), 2 details (song).
    fn code(self) -> u8 {
        match self {
            Self::AppName => 0,
            Self::Artist => 1,
            Self::Song => 2,
        }
    }
}

/// Sets which activity field Discord shows after "Listening to".
fn with_status_display(activity: Option<Value>, display: StatusDisplay) -> Option<Value> {
    activity.map(|mut value| {
        value["status_display_type"] = json!(display.code());
        value
    })
}

#[derive(Clone, Default)]
struct Update {
    enabled: bool,
    application_id: String,
    activity: Option<Value>,
}

pub struct Discord {
    updates: Option<watch::Sender<Update>>,
    statuses: mpsc::Receiver<String>,
    finished: mpsc::Receiver<()>,
    worker: Option<std::thread::JoinHandle<()>>,
    pub status: String,
}
impl Discord {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let (updates, receiver) = watch::channel(Update::default());
        let (status_tx, statuses) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = runtime {
                runtime.block_on(run(receiver, status_tx, ctx));
            }
            let _ = done.send(());
        });
        Self {
            updates: Some(updates),
            statuses,
            finished,
            worker: Some(worker),
            status: "Connecting to Discord…".into(),
        }
    }
    pub fn update(
        &mut self,
        enabled: bool,
        application_id: &str,
        track: Option<&Track>,
        state: &PlayerState,
        display: StatusDisplay,
    ) {
        let activity = with_status_display(activity(track, state, unix_time()), display);
        if let Some(updates) = &self.updates {
            updates.send_if_modified(|previous| {
                if previous.enabled == enabled
                    && previous.application_id == application_id
                    && equivalent(&previous.activity, &activity)
                {
                    false
                } else {
                    *previous = Update {
                        enabled,
                        application_id: application_id.into(),
                        activity,
                    };
                    true
                }
            });
        }
        while let Ok(status) = self.statuses.try_recv() {
            if status != self.status {
                crate::logs::info("discord", &status);
            }
            self.status = status;
        }
    }
}
impl Drop for Discord {
    fn drop(&mut self) {
        self.updates.take();
        if self
            .finished
            .recv_timeout(Duration::from_millis(1500))
            .is_ok()
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}

fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn valid_application_id(id: &str) -> bool {
    id.len() >= 17
        && id.len() <= 20
        && id.bytes().all(|c| c.is_ascii_digit())
        && id.parse::<u64>().is_ok_and(|id| id > 0)
}
fn text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    let mut end = value.len().min(128);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut value = value[..end].to_string();
    if value.chars().count() == 1 {
        value.push(' ');
    }
    value
}
/// Builds a Listening activity. While playing it carries start/end timestamps
/// for Discord's progress bar; a paused track stays visible without them.
/// Ads, buffering, errors, an ended track and an unloaded player clear it.
fn activity(track: Option<&Track>, state: &PlayerState, now: i64) -> Option<Value> {
    let track = track.filter(|track| valid_video_id(&track.id))?;
    let paused =
        !state.playing && !state.ended && state.duration.is_finite() && state.duration > 0.0;
    if !(state.playing || paused) || state.ad || state.buffering || state.error.is_some() {
        return None;
    }
    let title = text(&track.title, "YouTube Music");
    let artist = text(&track.artist, "Unknown artist");
    let cover_text = if track.album.trim().is_empty() {
        title.clone()
    } else {
        text(&track.album, "")
    };
    let link = format!("https://music.youtube.com/watch?v={}", track.id);
    let cover = reqwest::Url::parse(&track.cover)
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.as_str().len() <= 512
                && url.host_str().is_some_and(|host| {
                    host.ends_with(".ytimg.com")
                        || host.ends_with(".ggpht.com")
                        || host.ends_with(".googleusercontent.com")
                })
        })
        .map(|url| url.to_string())
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", track.id));
    let mut value = json!({"type":2,"details":title,"state":artist,"details_url":link,
        "assets":{"large_image":cover,"large_text":cover_text},
        "buttons":[{"label":"Listen on YouTube Music","url":link}]});
    if state.playing
        && state.position.is_finite()
        && state.duration.is_finite()
        && state.duration > 0.0
    {
        let position = state.position.clamp(0.0, state.duration);
        value["timestamps"] = json!({"start":now-position as i64,"end":now+(state.duration-position).max(0.0) as i64});
    }
    Some(value)
}
fn equivalent(left: &Option<Value>, right: &Option<Value>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            let clocks_match = ["start", "end"].into_iter().all(|key| {
                match (
                    left["timestamps"][key].as_i64(),
                    right["timestamps"][key].as_i64(),
                ) {
                    (Some(a), Some(b)) => (a - b).abs() <= 2,
                    (None, None) => true,
                    _ => false,
                }
            });
            clocks_match
                && [
                    "type",
                    "details",
                    "state",
                    "assets",
                    "buttons",
                    "status_display_type",
                ]
                .into_iter()
                .all(|key| left[key] == right[key])
        }
        _ => false,
    }
}

fn connect() -> io::Result<NamedPipeClient> {
    for index in 0..10 {
        if let Ok(pipe) = ClientOptions::new().open(format!(r"\\?\pipe\discord-ipc-{index}")) {
            return Ok(pipe);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotConnected,
        "Open Discord desktop to use Rich Presence.",
    ))
}
async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    op: u32,
    body: &[u8],
) -> io::Result<()> {
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RPC frame too large",
        ));
    }
    timeout(IO_TIMEOUT, async {
        writer.write_u32_le(op).await?;
        writer.write_u32_le(body.len() as u32).await?;
        writer.write_all(body).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Discord did not respond"))?
}
async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<(u32, Vec<u8>)> {
    let op = reader.read_u32_le().await?;
    let length = reader.read_u32_le().await? as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Discord frame too large",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok((op, body))
}
async fn handshake(stream: &mut (impl AsyncRead + AsyncWrite + Unpin), id: &str) -> io::Result<()> {
    write_frame(
        stream,
        0,
        &serde_json::to_vec(&json!({"v":1,"client_id":id}))?,
    )
    .await?;
    timeout(IO_TIMEOUT, async {
        loop {
            let (op, body) = read_frame(stream).await?;
            if op == 3 {
                write_frame(stream, 4, &body).await?;
                continue;
            }
            if op == 1 && serde_json::from_slice::<Value>(&body)?["evt"] == "READY" {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Discord rejected the Application ID. Check Settings.",
            ));
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Discord handshake timed out"))?
}
async fn set_activity(
    writer: &mut (impl AsyncWrite + Unpin),
    activity: &Option<Value>,
    nonce: u64,
) -> io::Result<()> {
    let body = serde_json::to_vec(
        &json!({"cmd":"SET_ACTIVITY","args":{"pid":std::process::id(),"activity":activity},"nonce":nonce.to_string()}),
    )?;
    write_frame(writer, 1, &body).await
}
struct ReaderTask(tokio::task::JoinHandle<()>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn session(
    stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    updates: &mut watch::Receiver<Update>,
    status: &impl Fn(&str),
) -> io::Result<()> {
    let id = updates.borrow().application_id.clone();
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (frames, mut incoming) = tokio::sync::mpsc::channel(8);
    // A dedicated reader retains partial frames when settings/playback change.
    let _reader = ReaderTask(tokio::spawn(async move {
        loop {
            let frame = read_frame(&mut reader).await;
            let failed = frame.is_err();
            if frames.send(frame).await.is_err() || failed {
                break;
            }
        }
    }));
    let mut last_sent: Option<Option<Value>> = None;
    let mut next_send = Instant::now();
    let mut nonce = 0;
    let mut awaiting_ack = None;
    loop {
        let latest = updates.borrow_and_update().clone();
        if !latest.enabled || latest.application_id != id {
            set_activity(&mut writer, &None, nonce + 1).await?;
            return Ok(());
        }
        let dirty = last_sent
            .as_ref()
            .is_none_or(|last| !equivalent(last, &latest.activity));
        // Clearing presence or progress (pause) is sent at once; other updates
        // respect the throttle that keeps us within Discord's rate limit.
        let has_progress = |activity: &Option<Value>| {
            activity
                .as_ref()
                .is_some_and(|a| a.get("timestamps").is_some())
        };
        let clears_progress = latest.activity.is_none()
            || (!has_progress(&latest.activity) && last_sent.as_ref().is_some_and(has_progress));
        if dirty && (clears_progress || Instant::now() >= next_send) {
            nonce += 1;
            set_activity(&mut writer, &latest.activity, nonce).await?;
            awaiting_ack = Some((
                nonce.to_string(),
                latest.activity.is_some(),
                Instant::now() + IO_TIMEOUT,
            ));
            last_sent = Some(latest.activity.clone());
            next_send = Instant::now() + Duration::from_secs(5);
        }
        let ack_deadline = awaiting_ack
            .as_ref()
            .map_or_else(Instant::now, |(_, _, deadline)| *deadline);
        tokio::select! {
            changed=updates.changed()=>if changed.is_err() {let _=set_activity(&mut writer,&None,nonce+1).await;return Ok(());},
            frame=incoming.recv()=> {
                let (op,body)=frame.ok_or_else(||io::Error::new(io::ErrorKind::BrokenPipe,"Discord closed the connection"))??;
                match op {
                    3=>write_frame(&mut writer,4,&body).await?,
                    1=> {
                        let value:Value=serde_json::from_slice(&body)?;
                        if value["evt"]=="ERROR" {return Err(io::Error::new(io::ErrorKind::PermissionDenied,"Discord rejected the presence update. Check the Application ID."));}
                        if let Some((nonce,sharing,_))=&awaiting_ack && value["cmd"]=="SET_ACTIVITY" && value["nonce"]==*nonce {
                            status(if *sharing {"Connected · sharing the current song"} else {"Connected · waiting for playback"});awaiting_ack=None;
                        }
                    },
                    2=>return Err(io::Error::new(io::ErrorKind::ConnectionAborted,"Discord disconnected. Reconnecting…")),
                    _=>{},
                }
            },
            _=tokio::time::sleep_until(next_send),if dirty=>{},
            _=tokio::time::sleep_until(ack_deadline),if awaiting_ack.is_some()=>return Err(io::Error::new(io::ErrorKind::TimedOut,"Discord did not acknowledge the presence update. Reconnecting…")),
        }
    }
}
async fn run(
    mut updates: watch::Receiver<Update>,
    statuses: mpsc::Sender<String>,
    ctx: eframe::egui::Context,
) {
    let status = |message: &str| {
        let _ = statuses.send(message.into());
        ctx.request_repaint();
    };
    loop {
        let latest = updates.borrow_and_update().clone();
        if !latest.enabled || !valid_application_id(&latest.application_id) {
            status(if latest.enabled {
                "Enter a valid Discord Application ID."
            } else {
                "Disabled"
            });
            if updates.changed().await.is_err() {
                return;
            }
            continue;
        }
        let result = async {
            let mut pipe = connect()?;
            handshake(&mut pipe, &latest.application_id).await?;
            status("Connected · waiting for playback");
            session(pipe, &mut updates, &status).await
        }
        .await;
        if let Err(error) = result {
            status(&error.to_string());
        }
        if updates.has_changed().is_err() {
            return;
        }
        if !updates.borrow().enabled || updates.borrow().application_id != latest.application_id {
            continue;
        }
        tokio::select! {_=sleep(Duration::from_secs(5))=>{},changed=updates.changed()=>if changed.is_err() {return;}}
    }
}

pub fn smoke() -> io::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut pipe = connect()?;
            handshake(&mut pipe, APPLICATION_ID).await?;
            set_activity(&mut pipe, &None, 1).await?;
            timeout(IO_TIMEOUT, async {
                loop {
                    let (op, body) = read_frame(&mut pipe).await?;
                    if op == 3 {
                        write_frame(&mut pipe, 4, &body).await?;
                        continue;
                    }
                    let value: Value = serde_json::from_slice(&body)?;
                    if op == 1
                        && value["cmd"] == "SET_ACTIVITY"
                        && value["evt"] != "ERROR"
                        && value["nonce"] == "1"
                    {
                        return Ok(());
                    }
                    return Err(io::Error::other("Discord rejected the activity command"));
                }
            })
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Discord activity acknowledgement timed out",
                )
            })?
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> Track {
        Track {
            id: "Fixture0001".into(),
            title: "𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬 🚀".into(),
            artist: "Nova Lane".into(),
            ..Default::default()
        }
    }
    #[test]
    fn presence_has_listening_metadata_and_seek_correct_timestamps() {
        let value = activity(
            Some(&track()),
            &PlayerState {
                playing: true,
                position: 30.,
                duration: 200.,
                ..Default::default()
            },
            1000,
        )
        .unwrap();
        assert_eq!(value["type"], 2);
        assert_eq!(value["state"], "Nova Lane");
        assert_eq!(value["timestamps"]["start"], 970);
        assert_eq!(value["timestamps"]["end"], 1170);
        assert_eq!(
            value["buttons"][0]["url"],
            "https://music.youtube.com/watch?v=Fixture0001"
        );
    }
    #[test]
    fn paused_track_stays_listed_without_progress_and_shows_album() {
        let value = activity(
            Some(&Track {
                album: "Album name".into(),
                ..track()
            }),
            &PlayerState {
                playing: false,
                position: 30.,
                duration: 200.,
                ..Default::default()
            },
            1000,
        )
        .unwrap();
        assert_eq!(value["type"], 2);
        assert_eq!(value["state"], "Nova Lane");
        assert_eq!(value["assets"]["large_text"], "Album name");
        assert!(value.get("timestamps").is_none());
    }
    #[test]
    fn ended_ads_buffering_and_idle_clear_presence() {
        for state in [
            PlayerState::default(),
            PlayerState {
                ended: true,
                duration: 200.,
                ..Default::default()
            },
            PlayerState {
                playing: true,
                ad: true,
                ..Default::default()
            },
            PlayerState {
                playing: true,
                buffering: true,
                ..Default::default()
            },
        ] {
            assert!(activity(Some(&track()), &state, 1000).is_none());
        }
        assert!(
            activity(
                None,
                &PlayerState {
                    playing: true,
                    ..Default::default()
                },
                1000
            )
            .is_none()
        );
    }
    #[test]
    fn clock_jitter_is_ignored_but_a_seek_updates_presence() {
        let state = PlayerState {
            playing: true,
            position: 30.,
            duration: 200.,
            ..Default::default()
        };
        let before = activity(Some(&track()), &state, 1000);
        assert!(equivalent(
            &before,
            &activity(
                Some(&track()),
                &PlayerState {
                    position: 31.,
                    ..state.clone()
                },
                1001
            )
        ));
        assert!(!equivalent(
            &before,
            &activity(
                Some(&track()),
                &PlayerState {
                    position: 100.,
                    ..state
                },
                1001
            )
        ));
    }
    #[test]
    fn the_status_line_can_show_the_artist_the_song_or_the_app_name() {
        let playing = PlayerState {
            playing: true,
            position: 30.,
            duration: 200.,
            ..Default::default()
        };
        let base = activity(Some(&track()), &playing, 1000);
        for (display, code) in [
            (StatusDisplay::AppName, 0),
            (StatusDisplay::Artist, 1),
            (StatusDisplay::Song, 2),
        ] {
            let value = with_status_display(base.clone(), display).unwrap();
            assert_eq!(value["status_display_type"], code);
            // The fields Discord picks from are still the artist and song.
            assert_eq!(value["state"], "Nova Lane");
            assert_eq!(value["details"], base.as_ref().unwrap()["details"]);
        }
        assert_eq!(StatusDisplay::default(), StatusDisplay::Artist);
        // Switching the choice re-sends the presence; clearing stays a clear.
        assert!(!equivalent(
            &with_status_display(base.clone(), StatusDisplay::Artist),
            &with_status_display(base, StatusDisplay::Song)
        ));
        assert!(with_status_display(None, StatusDisplay::Song).is_none());
    }
    #[test]
    fn text_truncation_preserves_unicode_and_id_validation_rejects_tokens() {
        let title = text(&"🚀".repeat(100), "");
        assert!(title.len() <= 128);
        assert_eq!(title.chars().count(), 32);
        assert!(valid_application_id(APPLICATION_ID));
        assert!(!valid_application_id("a-token"));
        assert!(!valid_application_id("99999999999999999999"));
    }
    #[tokio::test]
    async fn pause_and_disable_clear_immediately_even_during_a_partial_response() {
        let (client, mut server) = tokio::io::duplex(4096);
        let (updates, mut receiver) = watch::channel(Update {
            enabled: true,
            application_id: APPLICATION_ID.into(),
            activity: activity(
                Some(&track()),
                &PlayerState {
                    playing: true,
                    ..Default::default()
                },
                1000,
            ),
        });
        let task = tokio::spawn(async move { session(client, &mut receiver, &|_| {}).await });
        let (_, first) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        let first: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(first["cmd"], "SET_ACTIVITY");
        assert_eq!(first["args"]["activity"]["type"], 2);
        let reply =
            serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY","nonce":first["nonce"],"data":null}))
                .unwrap();
        server.write_u32_le(1).await.unwrap();
        updates.send_modify(|update| update.activity = None);
        let (_, clear) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        let clear: Value = serde_json::from_slice(&clear).unwrap();
        assert!(clear["args"]["activity"].is_null());
        server.write_u32_le(reply.len() as u32).await.unwrap();
        server.write_all(&reply).await.unwrap();
        write_frame(
            &mut server,
            1,
            &serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY","nonce":clear["nonce"],"data":null}))
                .unwrap(),
        )
        .await
        .unwrap();
        updates.send_modify(|update| update.enabled = false);
        let (_, clear) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        assert!(serde_json::from_slice::<Value>(&clear).unwrap()["args"]["activity"].is_null());
        timeout(IO_TIMEOUT, task).await.unwrap().unwrap().unwrap();
    }
    #[tokio::test]
    async fn pausing_removes_progress_without_waiting_for_the_throttle() {
        let playing = PlayerState {
            playing: true,
            position: 30.,
            duration: 200.,
            ..Default::default()
        };
        let (client, mut server) = tokio::io::duplex(4096);
        let (updates, mut receiver) = watch::channel(Update {
            enabled: true,
            application_id: APPLICATION_ID.into(),
            activity: activity(Some(&track()), &playing, 1000),
        });
        let task = tokio::spawn(async move { session(client, &mut receiver, &|_| {}).await });
        let ack = |nonce: &Value| {
            serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY","nonce":nonce,"data":null})).unwrap()
        };
        let (_, first) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        let first: Value = serde_json::from_slice(&first).unwrap();
        assert!(first["args"]["activity"]["timestamps"].is_object());
        write_frame(&mut server, 1, &ack(&first["nonce"]))
            .await
            .unwrap();
        let paused = PlayerState {
            playing: false,
            ..playing
        };
        updates.send_modify(|update| update.activity = activity(Some(&track()), &paused, 1001));
        // The regular throttle is five seconds; the pause must arrive well before it.
        let (_, pause) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        let pause: Value = serde_json::from_slice(&pause).unwrap();
        assert_eq!(pause["args"]["activity"]["type"], 2);
        assert!(pause["args"]["activity"].get("timestamps").is_none());
        write_frame(&mut server, 1, &ack(&pause["nonce"]))
            .await
            .unwrap();
        updates.send_modify(|update| update.enabled = false);
        let (_, clear) = timeout(IO_TIMEOUT, read_frame(&mut server))
            .await
            .unwrap()
            .unwrap();
        assert!(serde_json::from_slice::<Value>(&clear).unwrap()["args"]["activity"].is_null());
        timeout(IO_TIMEOUT, task).await.unwrap().unwrap().unwrap();
    }
    #[tokio::test]
    async fn framing_handles_partial_reads_ping_and_bounds() {
        let (mut client, mut server) = tokio::io::duplex(256);
        let peer = tokio::spawn(async move {
            assert_eq!(read_frame(&mut server).await.unwrap().0, 0);
            write_frame(&mut server, 3, b"ping").await.unwrap();
            assert_eq!(
                read_frame(&mut server).await.unwrap(),
                (4, b"ping".to_vec())
            );
            let body = br#"{"evt":"READY"}"#;
            server.write_all(&1u32.to_le_bytes()).await.unwrap();
            server
                .write_all(&(body.len() as u32).to_le_bytes())
                .await
                .unwrap();
            for byte in body {
                server.write_all(&[*byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
            server.write_u32_le(1).await.unwrap();
            server.write_u32_le((MAX_FRAME + 1) as u32).await.unwrap();
        });
        handshake(&mut client, APPLICATION_ID).await.unwrap();
        assert_eq!(
            read_frame(&mut client).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        peer.await.unwrap();
    }
}
