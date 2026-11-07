use eframe::egui;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

pub const BROWSER_ARGS: &str = "--autoplay-policy=no-user-gesture-required";

pub enum PlayerCommand {
    Play { generation: u64, video_id: String },
    Toggle,
    Volume(f64),
    Stop,
    Video(bool),
}
#[derive(Default, Clone, PartialEq)]
pub struct PlayerState {
    pub seek_ack: u64,
    pub title: String,
    pub transition: bool,
    pub generation: u64,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub buffering: bool,
    pub ended: bool,
    pub ad: bool,
    pub video_width: u32,
    pub video_height: u32,
    pub video_available: bool,
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeEvent {
    Ready,
    Trace {
        phase: TracePhase,
        position: f64,
        target: Option<f64>,
        video: bool,
        ready: u8,
        seeking: bool,
        api_state: i8,
    },
    Shortcut {
        action: Shortcut,
    },
    State {
        seek_ack: u64,
        title: String,
        transition: bool,
        generation: u64,
        video_id: String,
        position: f64,
        duration: f64,
        playing: bool,
        buffering: bool,
        ended: bool,
        ad: bool,
        video_width: u32,
        video_height: u32,
        video_available: bool,
        error: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Shortcut {
    Toggle,
    Quit,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum TracePhase {
    ModeStarted,
    ModeSettled,
    SeekQueued,
    SeekApplied,
    SeekWaiting,
}

fn playback_origin(uri: &str) -> bool {
    reqwest::Url::parse(uri)
        .is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some("music.youtube.com"))
}

pub struct Player {
    webview: Option<WebView>,
    _context: Option<WebContext>,
    incoming: Receiver<BridgeEvent>,
    updates: Sender<PlayerState>,
    pub rx: Receiver<PlayerState>,
    ready: Cell<bool>,
    pending: RefCell<Vec<Value>>,
    generation: Cell<u64>,
    requested: Cell<Option<Instant>>,
    volume: Cell<f64>,
    video: Cell<bool>,
    last_bounds: Cell<Option<[i32; 4]>>,
    quit_requested: Cell<bool>,
    seek_revision: Cell<u64>,
    pub session_rx: Receiver<crate::account::Capture>,
    session_tx: Sender<crate::account::Capture>,
    session_checked: Cell<Option<Instant>>,
    repaint: egui::Context,
}
impl Player {
    pub fn start(cc: &eframe::CreationContext<'_>, volume: f64) -> Self {
        let (events, incoming) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        let (session_tx, session_rx) = mpsc::channel();
        let mut result = Self {
            webview: None,
            _context: None,
            incoming,
            updates,
            rx,
            ready: Cell::new(false),
            pending: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            requested: Cell::new(None),
            volume: Cell::new(volume.clamp(0.0, 1.0)),
            video: Cell::new(false),
            last_bounds: Cell::new(None),
            quit_requested: Cell::new(false),
            seek_revision: Cell::new(0),
            session_rx,
            session_tx,
            session_checked: Cell::new(None),
            repaint: cc.egui_ctx.clone(),
        };
        if crate::preview::active() {
            return result;
        }
        let build = (|| -> Result<_, Box<dyn std::error::Error>> {
            let mut context = WebContext::new(Some(rustified_core::session::profile_dir()?));
            let repaint = cc.egui_ctx.clone();
            let webview = WebViewBuilder::new_with_web_context(&mut context)
                .with_additional_browser_args(BROWSER_ARGS)
                .with_browser_accelerator_keys(false)
                .with_default_context_menus(false)
                .with_background_color((0, 0, 0, 255))
                .with_bounds(wry::Rect {
                    position: wry::dpi::PhysicalPosition::new(-200, -200).into(),
                    size: wry::dpi::PhysicalSize::new(160, 90).into(),
                })
                .with_initialization_script(format!(
                    "window.__rustifiedInitialVolume={};\n{}",
                    result.volume.get(),
                    include_str!("media_bridge.js")
                ))
                .with_ipc_handler(move |request| {
                    if !playback_origin(&request.uri().to_string()) || request.body().len() > 16384
                    {
                        return;
                    }
                    if let Ok(event) = serde_json::from_str::<BridgeEvent>(request.body()) {
                        let _ = events.send(event);
                        repaint.request_repaint();
                    }
                })
                .with_navigation_handler(|url| url == "about:blank" || playback_origin(&url))
                .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                .with_url("https://music.youtube.com/")
                .build_as_child(cc)?;
            Ok((webview, context))
        })();
        match build {
            Ok((webview, context)) => {
                result.webview = Some(webview);
                result._context = Some(context);
            }
            Err(_) => {
                let _ = result.updates.send(PlayerState { error:Some("The YouTube playback engine could not start. Install Microsoft WebView2 Runtime and restart RustifiedYoutube.".into()),..Default::default() });
            }
        }
        result
    }
    pub fn is_ready(&self) -> bool {
        self.ready.get()
    }
    pub fn take_quit(&self) -> bool {
        self.quit_requested.replace(false)
    }
    pub fn pump(&self) {
        while let Ok(event) = self.incoming.try_recv() {
            match event {
                BridgeEvent::Shortcut {
                    action: Shortcut::Toggle,
                } => self.send(PlayerCommand::Toggle),
                BridgeEvent::Shortcut {
                    action: Shortcut::Quit,
                } => self.quit_requested.set(true),
                BridgeEvent::Ready => {
                    self.ready.set(true);
                    self.session_checked.set(Some(Instant::now()));
                    if rustified_core::session::native_enabled() {
                        self.capture_session(0);
                    }
                    if std::env::args().any(|arg| arg == "--smoke-stream") {
                        self.eval(&json!({"op":"diagnostic"}));
                    }
                    self.eval(&json!({"op":"volume","value":self.volume.get()}));
                    self.eval(&json!({"op":"mode","video":self.video.get()}));
                    for command in self.pending.borrow_mut().drain(..) {
                        self.eval(&command);
                    }
                }
                BridgeEvent::Trace {
                    phase,
                    position,
                    target,
                    video,
                    ready,
                    seeking,
                    api_state,
                } => {
                    if std::env::args().any(|arg| arg == "--smoke-stream") {
                        println!(
                            "Media trace: {phase:?}, position {position:.2}, seek {target:?}, video {video}, ready {ready}, seeking {seeking}, state {api_state}"
                        );
                    }
                }
                BridgeEvent::State {
                    seek_ack,
                    title,
                    transition,
                    generation,
                    video_id,
                    position,
                    duration,
                    playing,
                    buffering,
                    ended,
                    ad,
                    video_width,
                    video_height,
                    video_available,
                    error,
                } => {
                    if generation != self.generation.get()
                        || !position.is_finite()
                        || !duration.is_finite()
                        || position < 0.0
                        || duration < 0.0
                    {
                        continue;
                    }
                    if !video_id.is_empty() && !rustified_core::model::valid_video_id(&video_id) {
                        continue;
                    }
                    if playing || error.is_some() || (!buffering && duration > 0.) {
                        self.requested.set(None);
                    }
                    let _ = self.updates.send(PlayerState {
                        seek_ack,
                        title,
                        transition,
                        generation,
                        position,
                        duration,
                        playing,
                        buffering,
                        ended,
                        ad,
                        video_width,
                        video_height,
                        video_available,
                        error,
                    });
                }
            }
        }
        if self
            .requested
            .get()
            .is_some_and(|started| started.elapsed() > Duration::from_secs(40))
        {
            self.requested.set(None);
            let _ = self.updates.send(PlayerState { generation:self.generation.get(),error:Some("YouTube did not start this track. Check your connection and try again. If Google requests sign-in, use Continue with Google.".into()),..Default::default() });
        }
        if self.ready.get()
            && self
                .session_checked
                .get()
                .is_none_or(|checked| checked.elapsed() > Duration::from_secs(300))
        {
            self.session_checked.set(Some(Instant::now()));
            if rustified_core::session::native_enabled() {
                self.capture_session(0);
            }
        }
    }
    pub fn capture_session(&self, id: u64) {
        let tx = self.session_tx.clone();
        let repaint = self.repaint.clone();
        let result = (|| -> windows::core::Result<()> {
            let webview = self.webview.as_ref().ok_or_else(|| {
                windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))
            })?;
            let core = unsafe { webview.controller().CoreWebView2()? };
            rustified_core::browser_session::capture(&core, move |result| {
                let _ = tx.send(crate::account::Capture { id, result });
                repaint.request_repaint();
            })
        })();
        if result.is_err() {
            let _ = self.session_tx.send(crate::account::Capture {
                id,
                result: Err(()),
            });
            self.repaint.request_repaint();
        }
    }
    fn eval(&self, command: &Value) {
        if let Some(webview) = &self.webview {
            let _ =
                webview.evaluate_script(&format!("window.__rustifiedMedia?.command({command});"));
        }
    }
    pub fn send(&self, command: PlayerCommand) {
        let message = match command {
            PlayerCommand::Play {
                generation,
                video_id,
            } => {
                if !rustified_core::model::valid_video_id(&video_id) {
                    return;
                }
                self.generation.set(generation);
                self.requested.set(Some(Instant::now()));
                self.pending.borrow_mut().clear();
                json!({"op":"play","generation":generation,"id":video_id})
            }
            PlayerCommand::Toggle => json!({"op":"toggle"}),
            PlayerCommand::Stop => {
                self.requested.set(None);
                json!({"op":"pause"})
            }
            PlayerCommand::Volume(value) => {
                self.volume.set(value.clamp(0.0, 1.0));
                json!({"op":"volume","value":self.volume.get()})
            }
            PlayerCommand::Video(video) => {
                self.video.set(video);
                json!({"op":"mode","video":video})
            }
        };
        if self.ready.get() {
            self.eval(&message);
        } else {
            self.pending.borrow_mut().push(message);
        }
    }
    pub fn seek(&self, seconds: f64) -> u64 {
        let revision = self.seek_revision.get() + 1;
        self.seek_revision.set(revision);
        let message = json!({"op":"seek","value":seconds.max(0.0),"revision":revision});
        if self.ready.get() {
            self.eval(&message);
        } else {
            self.pending.borrow_mut().push(message);
        }
        revision
    }
    pub fn place_video(&self, rect: Option<egui::Rect>, pixels_per_point: f32) {
        let bounds = rect
            .map(|rect| {
                [
                    (rect.left() * pixels_per_point).round() as i32,
                    (rect.top() * pixels_per_point).round() as i32,
                    (rect.width() * pixels_per_point).round().max(1.0) as i32,
                    (rect.height() * pixels_per_point).round().max(1.0) as i32,
                ]
            })
            .unwrap_or([-200, -200, 160, 90]);
        if self.last_bounds.get() == Some(bounds) {
            return;
        }
        self.last_bounds.set(Some(bounds));
        if let Some(webview) = &self.webview {
            let _ = webview.set_bounds(wry::Rect {
                position: wry::dpi::PhysicalPosition::new(bounds[0], bounds[1]).into(),
                size: wry::dpi::PhysicalSize::new(bounds[2] as u32, bounds[3] as u32).into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bridge_accepts_only_the_exact_music_origin() {
        assert!(super::playback_origin(
            "https://music.youtube.com/watch?v=Fixture0001"
        ));
        for url in [
            "http://music.youtube.com/",
            "https://music.youtube.com.example.org/",
            "https://accounts.google.com/",
            "file:///C:/test",
            "https://music.youtube.com@evil.example/",
        ] {
            assert!(!super::playback_origin(url));
        }
    }
    #[test]
    fn bridge_rejects_unknown_commands_and_non_finite_positions() {
        assert!(
            serde_json::from_str::<super::BridgeEvent>(
                r#"{"type":"shortcut","action":"launch_process"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<super::BridgeEvent>(r#"{"type":"state","position":NaN}"#)
                .is_err()
        );
    }
}
