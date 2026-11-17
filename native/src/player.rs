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
    #[serde(skip_deserializing)]
    Captured {
        capture: crate::account::Capture,
        document_id: Option<String>,
        identity: crate::playback_session::Identity,
    },
    #[serde(skip_deserializing)]
    Navigation {
        destination: crate::navigation::Destination,
    },
    #[serde(skip_deserializing)]
    Popup {
        url: String,
    },
    Stalled {
        document_id: String,
        generation: u64,
        video_id: String,
        status: String,
    },
    Ready {
        document_id: String,
    },
    Session {
        document_id: String,
        #[serde(flatten)]
        identity: crate::playback_session::Identity,
    },
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
        document_id: String,
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
    Next,
    Previous,
}

/// Queue navigation requested from the page's media session (keyboard media
/// keys, headset buttons, the Windows media flyout); the app owns the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAction {
    Next,
    Previous,
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
    crate::navigation::destination(uri) == Some(crate::navigation::Destination::Music)
}

pub struct Player {
    webview: Option<WebView>,
    mixer: Option<crate::mixer::Mixer>,
    _context: Option<WebContext>,
    incoming: Receiver<BridgeEvent>,
    events: Sender<BridgeEvent>,
    updates: Sender<PlayerState>,
    pub rx: Receiver<PlayerState>,
    ready: Cell<bool>,
    generation: Cell<u64>,
    requested: Cell<Option<Instant>>,
    loading_since: Cell<Option<Instant>>,
    volume: Cell<f64>,
    video: Cell<bool>,
    last_bounds: Cell<Option<[i32; 4]>>,
    quit_requested: Cell<bool>,
    media_action: Cell<Option<MediaAction>>,
    seek_revision: Cell<u64>,
    pub session_rx: Receiver<crate::account::Capture>,
    session_tx: Sender<crate::account::Capture>,
    session_refresh_at: Cell<Option<Instant>>,
    repaint: egui::Context,
    identity: Cell<crate::playback_session::Identity>,
    expected_account: Cell<bool>,
    selected_account: Cell<Option<u8>>,
    session_recheck_attempted: Cell<bool>,
    page_reload_attempted: Cell<bool>,
    document: RefCell<Option<String>>,
    resume: RefCell<Option<Value>>,
    restoring: Cell<bool>,
    waiting_restore: Cell<bool>,
    login_open: Cell<bool>,
    login_completed: Cell<bool>,
    login_failed: Cell<bool>,
    google_sign_out_pending: Cell<Option<Instant>>,
    google_sign_out_result: RefCell<Option<Result<(), String>>>,
    suppress_login: Cell<bool>,
    recovered: Cell<bool>,
    prefer_direct: Cell<bool>,
    last_state: RefCell<PlayerState>,
}
impl Player {
    pub fn start(cc: &eframe::CreationContext<'_>, volume: f64) -> Self {
        let (events, incoming) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        let (session_tx, session_rx) = mpsc::channel();
        let saved = rustified_core::session::native_enabled()
            .then(|| rustified_core::session::load_session().ok().flatten())
            .flatten();
        let mut result = Self {
            webview: None,
            mixer: None,
            _context: None,
            incoming,
            events: events.clone(),
            updates,
            rx,
            ready: Cell::new(false),
            generation: Cell::new(0),
            requested: Cell::new(None),
            loading_since: Cell::new(Some(Instant::now())),
            volume: Cell::new(volume.clamp(0.0, 1.0)),
            video: Cell::new(false),
            last_bounds: Cell::new(None),
            quit_requested: Cell::new(false),
            media_action: Cell::new(None),
            seek_revision: Cell::new(0),
            session_rx,
            session_tx,
            session_refresh_at: Cell::new(None),
            repaint: cc.egui_ctx.clone(),
            identity: Cell::new(Default::default()),
            expected_account: Cell::new(saved.is_some()),
            selected_account: Cell::new(saved.and_then(|session| session.auth_user)),
            session_recheck_attempted: Cell::new(false),
            page_reload_attempted: Cell::new(false),
            document: RefCell::new(None),
            resume: RefCell::new(None),
            restoring: Cell::new(false),
            waiting_restore: Cell::new(false),
            login_open: Cell::new(false),
            login_completed: Cell::new(false),
            login_failed: Cell::new(false),
            google_sign_out_pending: Cell::new(None),
            google_sign_out_result: RefCell::new(None),
            suppress_login: Cell::new(false),
            recovered: Cell::new(false),
            prefer_direct: Cell::new(false),
            last_state: RefCell::new(Default::default()),
        };
        if crate::preview::active() {
            return result;
        }
        if crate::account::smoke_active() {
            println!("Playback profile initialization started");
        }
        let build = (|| -> Result<_, Box<dyn std::error::Error>> {
            let mut context = WebContext::new(Some(rustified_core::session::profile_dir()?));
            if crate::account::smoke_active() {
                println!("Persistent WebView context initialized");
            }
            let repaint = cc.egui_ctx.clone();
            let nav_repaint = repaint.clone();
            let popup_repaint = repaint.clone();
            let navigation_events = events.clone();
            let popup_events = events.clone();
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
                .with_navigation_handler(move |url| {
                    if url == "about:blank" {
                        return true;
                    }
                    if let Some(destination) = crate::navigation::destination(&url) {
                        let _ = navigation_events.send(BridgeEvent::Navigation { destination });
                        nav_repaint.request_repaint();
                        true
                    } else {
                        false
                    }
                })
                .with_new_window_req_handler(move |url, _| {
                    if crate::navigation::destination(&url).is_some() {
                        let _ = popup_events.send(BridgeEvent::Popup { url });
                        popup_repaint.request_repaint();
                    }
                    wry::NewWindowResponse::Deny
                })
                .with_url(rustified_core::session::music_url(
                    result.selected_account.get(),
                ))
                .build_as_child(cc)?;
            if crate::account::smoke_active() {
                println!("Playback WebView created");
            }
            Ok((webview, context))
        })();
        match build {
            Ok((webview, context)) => {
                result.webview = Some(webview);
                result.mixer = crate::mixer::Mixer::start();
                result.sync_audio_identity();
                result._context = Some(context);
                if crate::account::smoke_active() {
                    println!("Playback startup completed");
                }
            }
            Err(_) => {
                let _ = result.updates.send(PlayerState { error:Some("The YouTube playback engine could not start. Install Microsoft WebView2 Runtime and restart RustifiedYoutube.".into()),..Default::default() });
            }
        }
        result
    }
    fn sync_audio_identity(&self) {
        if let (Some(webview), Some(mixer)) = (&self.webview, &self.mixer) {
            let _ = (|| -> windows::core::Result<()> {
                let core = unsafe { webview.controller().CoreWebView2()? };
                let mut browser = 0;
                unsafe {
                    core.BrowserProcessId(&mut browser)?;
                }
                mixer.set_browser(browser);
                Ok(())
            })();
        }
    }
    pub fn mixer_verified(&self) -> bool {
        self.mixer.as_ref().is_some_and(|mixer| mixer.verified())
    }
    pub fn is_ready(&self) -> bool {
        self.ready.get()
    }
    pub fn take_quit(&self) -> bool {
        self.quit_requested.replace(false)
    }
    pub fn take_media_action(&self) -> Option<MediaAction> {
        self.media_action.take()
    }
    pub fn pump(&self) {
        while let Ok(event) = self.incoming.try_recv() {
            match event {
                BridgeEvent::Captured {
                    mut capture,
                    document_id,
                    identity,
                } => {
                    if !crate::playback_session::capture_is_current(
                        document_id.as_deref(),
                        self.document.borrow().as_deref(),
                        identity,
                        self.identity.get(),
                    ) {
                        // Resolve foreground requests without allowing an old
                        // document/account to overwrite the current session.
                        capture.result = Err(());
                    } else {
                        let interval = if matches!(capture.result, Ok(Some(_))) {
                            300
                        } else {
                            15
                        };
                        self.session_refresh_at
                            .set(Some(Instant::now() + Duration::from_secs(interval)));
                    }
                    let _ = self.session_tx.send(capture);
                }
                BridgeEvent::Navigation { destination } => {
                    self.suspend();
                    if destination == crate::navigation::Destination::Account {
                        // Google's SSO/consent handoffs also visit account
                        // hosts. Navigation alone cannot establish sign-out.
                        if !self.suppress_login.get() {
                            self.login_open.set(true);
                        }
                    }
                    if crate::account::smoke_active()
                        || std::env::args().any(|arg| arg == "--smoke-stream")
                    {
                        println!(
                            "Media navigation: {}",
                            if destination == crate::navigation::Destination::Music {
                                "music"
                            } else {
                                "account"
                            }
                        );
                    }
                }
                BridgeEvent::Popup { url } => {
                    if let Some(destination) = crate::navigation::destination(&url) {
                        if destination == crate::navigation::Destination::Account {
                            self.login_open.set(true);
                            self.suppress_login.set(false);
                        }
                        self.suspend();
                        if let Some(webview) = &self.webview {
                            let _ = webview.load_url(&url);
                        }
                    }
                }
                BridgeEvent::Stalled {
                    document_id,
                    generation,
                    video_id,
                    status,
                } => {
                    if self.document.borrow().as_ref() != Some(&document_id)
                        || generation != self.generation.get()
                        || !rustified_core::model::valid_video_id(&video_id)
                        || self
                            .resume
                            .borrow()
                            .as_ref()
                            .is_none_or(|resume| resume["id"] != video_id)
                        || self.login_open.get()
                    {
                        continue;
                    }
                    if status == "LOGIN_REQUIRED" {
                        // A track's playability response isn't the account's
                        // identity (age/region restrictions also use this code).
                        // Retry the authenticated document once, then report a
                        // track error while preserving the confirmed account.
                        if self.allowed() && !self.recovered.replace(true) {
                            self.reconnect();
                        } else {
                            self.eval(&json!({"op":"pause"}));
                            self.notify_error(if self.needs_login() {
                                "Google requests sign-in. Choose Continue with Google."
                            } else {
                                "YouTube requires additional access for this track. Try another result. Your account is still saved."
                            });
                        }
                    } else if self.allowed() && !self.recovered.replace(true) {
                        self.prefer_direct.set(true);
                        if std::env::args().any(|arg| arg == "--smoke-stream") {
                            println!("Recovering stalled navigation using the requested watch URL");
                        }
                        self.reconnect();
                    } else {
                        self.notify_error("YouTube couldn't load this track. Use Retry playback or try another result.");
                    }
                }
                BridgeEvent::Session {
                    document_id,
                    identity,
                } => {
                    if self.document.borrow().as_ref() != Some(&document_id) {
                        continue;
                    }
                    let previous = self.identity.get();
                    let identity = crate::playback_session::observed(previous, identity);
                    self.identity.set(identity);
                    if self.google_sign_out_pending.get().is_some() && identity.signed_in.is_some()
                    {
                        self.google_sign_out_pending.set(None);
                        if identity.signed_in == Some(false) {
                            self.selected_account.set(None);
                            self.resume.borrow_mut().take();
                            self.requested.set(None);
                            self.restoring.set(false);
                            self.waiting_restore.set(false);
                            self.login_open.set(false);
                            self.login_completed.set(false);
                            self.eval(&json!({"op":"pause"}));
                            *self.google_sign_out_result.borrow_mut() = Some(Ok(()));
                            continue;
                        } else {
                            *self.google_sign_out_result.borrow_mut() =
                                Some(Err("Google still reports this app as signed in.".into()));
                        }
                    }
                    if identity.signed_in == Some(true) {
                        self.session_recheck_attempted.set(false);
                        if let Some(index) = identity.auth_user {
                            self.selected_account.set(Some(index));
                        }
                        if rustified_core::session::native_enabled() {
                            self.expected_account.set(true);
                            if previous != identity {
                                self.session_refresh_at
                                    .set(Some(Instant::now() + Duration::from_secs(300)));
                                self.capture_session(0);
                            }
                        }
                    } else if identity.signed_in == Some(false)
                        && !self.login_open.get()
                        && self.expected_account.get()
                        && !self.suppress_login.get()
                        && !self.session_recheck_attempted.replace(true)
                    {
                        // Retry the existing browser profile once before a
                        // guest bootstrap is surfaced as a signed-out player.
                        self.reconnect();
                        continue;
                    }
                    if std::env::args().any(|arg| arg == "--smoke-player-session")
                        && identity.signed_in.is_some()
                    {
                        println!(
                            "Playback document authenticated: {}",
                            identity.signed_in == Some(true)
                        );
                        if identity.signed_in != Some(true) {
                            crate::STREAM_CHECK_FAILED
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        self.quit_requested.set(true);
                    }
                    if identity.signed_in == Some(true) && self.login_open.replace(false) {
                        self.login_completed.set(true);
                        self.suppress_login.set(false);
                        if self.prefer_direct.get() && self.resume.borrow().is_some() {
                            self.reconnect();
                            continue;
                        }
                    }
                    if self.allowed() {
                        self.flush();
                    } else {
                        self.eval(&json!({"op":"pause"}));
                        self.requested.set(None);
                        self.restoring.set(true);
                    }
                }
                BridgeEvent::Shortcut {
                    action: Shortcut::Toggle,
                } => self.send(PlayerCommand::Toggle),
                BridgeEvent::Shortcut {
                    action: Shortcut::Quit,
                } => self.quit_requested.set(true),
                BridgeEvent::Shortcut {
                    action: Shortcut::Next,
                } => self.media_action.set(Some(MediaAction::Next)),
                BridgeEvent::Shortcut {
                    action: Shortcut::Previous,
                } => self.media_action.set(Some(MediaAction::Previous)),
                BridgeEvent::Ready { document_id } => {
                    if document_id.len() > 128 {
                        continue;
                    }
                    *self.document.borrow_mut() = Some(document_id);
                    self.identity.set(Default::default());
                    if self.resume.borrow().is_some() {
                        self.restoring.set(true);
                    }
                    self.ready.set(true);
                    self.sync_audio_identity();
                    if std::env::args().any(|arg| arg == "--smoke-stream") {
                        self.eval(&json!({"op":"diagnostic"}));
                    }
                    if std::env::args().any(|arg| arg == "--smoke-recovery")
                        && !self.recovered.get()
                    {
                        self.eval(&json!({"op":"recovery_probe"}));
                    }
                    self.eval(&json!({"op":"volume","value":self.volume.get()}));
                    self.eval(&json!({"op":"mode","video":self.video.get()}));
                    if self.allowed() {
                        self.flush();
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
                    document_id,
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
                    if self.document.borrow().as_ref() != Some(&document_id)
                        || !self.allowed()
                        || generation != self.generation.get()
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
                    if self.waiting_restore.get()
                        && error.is_none()
                        && (transition || buffering || duration <= 0.)
                    {
                        continue;
                    }
                    self.waiting_restore.set(false);
                    if !ad
                        && !transition
                        && !buffering
                        && error.is_none()
                        && duration > 0.
                        && let Some(resume) = self.resume.borrow_mut().as_mut()
                    {
                        resume["position"] = json!(position);
                        resume["paused"] = json!(!playing);
                        resume["revision"] = json!(seek_ack);
                    }
                    if playing || error.is_some() || (!buffering && duration > 0.) {
                        self.requested.set(None);
                    }
                    if playing && !ad && !transition && !buffering && error.is_none() {
                        self.prefer_direct.set(false);
                        self.recovered.set(false);
                    }
                    let state = PlayerState {
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
                    };
                    *self.last_state.borrow_mut() = state.clone();
                    let _ = self.updates.send(state);
                }
            }
        }
        if self
            .requested
            .get()
            .is_some_and(|started| started.elapsed() > Duration::from_secs(40))
        {
            self.requested.set(None);
            self.notify_error(
                "YouTube did not start this track. Try Retry playback or another result.",
            );
        }
        if self.ready.get() && self.authenticated().is_some() && !self.login_open.get() {
            self.loading_since.set(None);
            self.page_reload_attempted.set(false);
        }
        if self
            .loading_since
            .get()
            .is_some_and(|started| started.elapsed() > Duration::from_secs(25))
            && (!self.login_open.get() || self.on_music_page())
        {
            self.loading_since.set(None);
            if self.login_open.replace(false) {
                self.login_failed.set(true);
            }
            if !self.page_reload_attempted.replace(true) {
                self.reconnect();
                return;
            }
            if crate::account::smoke_active()
                && let Some(webview) = &self.webview
            {
                let _ = webview.evaluate_script_with_callback(
                    "JSON.stringify({origin:location.origin,ready:document.readyState,bridge:!!window.__rustifiedMedia,playerBar:!!document.querySelector('ytmusic-player-bar'),configuration:!!window.ytcfg,signedIn:window.ytcfg?.get?.('LOGGED_IN')===true})",
                    |result| println!("Playback bootstrap diagnostic: {result}"),
                );
            }
            self.notify_error(
                "The playback page couldn't finish loading. Try Retry playback or Google sign-in.",
            );
        }
        if self.authenticated() == Some(true)
            && rustified_core::session::native_enabled()
            && let Some(refresh_at) = self.session_refresh_at.get()
        {
            let remaining = refresh_at.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.session_refresh_at
                    .set(Some(Instant::now() + Duration::from_secs(300)));
                self.capture_session(0);
            } else {
                self.repaint.request_repaint_after(remaining);
            }
        }
        if self.loading_since.get().is_some() || self.google_sign_out_pending.get().is_some() {
            self.repaint.request_repaint_after(Duration::from_secs(1));
        }
        if self
            .google_sign_out_pending
            .get()
            .is_some_and(|started| started.elapsed() > Duration::from_secs(30))
        {
            self.google_sign_out_pending.set(None);
            *self.google_sign_out_result.borrow_mut() = Some(Err(
                "Google sign-out was not confirmed. Your saved native account was kept.".into(),
            ));
        }
    }
    pub fn capture_session(&self, id: u64) {
        if self.authenticated() != Some(true) {
            if id != 0 {
                let _ = self.session_tx.send(crate::account::Capture {
                    id,
                    result: Err(()),
                });
            }
            return;
        }
        let tx = self.events.clone();
        let document_id = self.document.borrow().clone();
        let identity = self.identity.get();
        let repaint = self.repaint.clone();
        let result = (|| -> windows::core::Result<()> {
            let webview = self.webview.as_ref().ok_or_else(|| {
                windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))
            })?;
            let core = unsafe { webview.controller().CoreWebView2()? };
            let auth_user = identity.auth_user.or(self.selected_account.get());
            rustified_core::browser_session::capture(&core, auth_user, move |result| {
                let _ = tx.send(BridgeEvent::Captured {
                    capture: crate::account::Capture { id, result },
                    document_id,
                    identity,
                });
                repaint.request_repaint();
            })
        })();
        if result.is_err() {
            self.session_refresh_at
                .set(Some(Instant::now() + Duration::from_secs(15)));
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
    pub fn authenticated(&self) -> Option<bool> {
        self.identity.get().signed_in
    }
    pub fn is_restoring(&self) -> bool {
        self.restoring.get() || self.waiting_restore.get()
    }
    pub fn needs_login(&self) -> bool {
        self.expected_account.get() && self.authenticated() == Some(false)
    }
    fn allowed(&self) -> bool {
        !self.login_open.get()
            && crate::playback_session::allowed(self.expected_account.get(), self.identity.get())
    }
    pub fn require_account(&self, expected: bool) {
        self.expected_account.set(expected);
        if !self.allowed() {
            self.eval(&json!({"op":"pause"}));
            self.requested.set(None);
        } else if self.ready.get() {
            self.flush();
        }
    }
    fn flush(&self) {
        self.eval(&json!({"op":"volume","value":self.volume.get()}));
        if self.restoring.replace(false)
            && let Some(resume) = self.resume.borrow().as_ref()
        {
            self.eval(&json!({"op":"mode","video":self.video.get()}));
            self.waiting_restore.set(true);
            self.eval(resume);
            self.requested.set(Some(Instant::now()));
        }
    }
    fn notify_error(&self, message: &str) {
        self.requested.set(None);
        let mut state = self.last_state.borrow().clone();
        state.generation = self.generation.get();
        state.playing = false;
        state.buffering = false;
        state.error = Some(message.into());
        let _ = self.updates.send(state);
    }
    fn suspend(&self) {
        self.loading_since.set(Some(Instant::now()));
        self.ready.set(false);
        self.identity.set(Default::default());
        *self.document.borrow_mut() = None;
        self.restoring.set(true);
        self.requested.set(None);
    }
    pub fn login_open(&self) -> bool {
        self.login_open.get()
    }
    pub fn take_login_completed(&self) -> bool {
        self.login_completed.replace(false)
    }
    pub fn take_login_failed(&self) -> bool {
        self.login_failed.replace(false)
    }
    fn on_music_page(&self) -> bool {
        self.webview
            .as_ref()
            .and_then(|webview| webview.url().ok())
            .is_some_and(|url| playback_origin(&url))
    }
    pub fn begin_login(&self) {
        self.eval(&json!({"op":"pause"}));
        self.suspend();
        self.suppress_login.set(false);
        self.session_recheck_attempted.set(false);
        self.page_reload_attempted.set(false);
        self.login_open.set(true);
        if let Some(webview) = &self.webview {
            let _ = webview.load_url(&crate::navigation::sign_in_url());
        }
    }
    pub fn begin_google_sign_out(&self) -> Result<(), String> {
        let Some(webview) = &self.webview else {
            return Err("The Google sign-in browser is not available.".into());
        };
        if self.google_sign_out_pending.get().is_some() {
            return Err("Google sign-out is already in progress.".into());
        }
        self.eval(&json!({"op":"pause"}));
        self.suspend();
        self.login_open.set(false);
        self.login_completed.set(false);
        self.suppress_login.set(true);
        self.google_sign_out_pending.set(Some(Instant::now()));
        if let Err(error) = webview.load_url(&crate::navigation::sign_out_url()) {
            self.google_sign_out_pending.set(None);
            return Err(format!("Could not open Google sign-out: {error}"));
        }
        Ok(())
    }
    pub fn take_google_sign_out_result(&self) -> Option<Result<(), String>> {
        self.google_sign_out_result.borrow_mut().take()
    }
    pub fn cancel_google_sign_out(&self) {
        self.google_sign_out_pending.set(None);
        self.suppress_login.set(false);
        if !self.ready.get() {
            self.reconnect();
        }
    }
    pub fn cancel_login(&self) {
        self.login_completed.set(false);
        self.login_open.set(false);
        self.suppress_login.set(true);
        self.reconnect();
    }
    pub fn reconnect(&self) {
        self.eval(&json!({"op":"pause"}));
        self.suspend();
        let index = self.selected_account.get();
        let url = {
            let mut resume = self.resume.borrow_mut();
            resume
                .as_mut()
                .and_then(|resume| {
                    resume["direct"] = json!(true);
                    resume["id"]
                        .as_str()
                        .and_then(|id| crate::navigation::watch_url(id, index))
                })
                .unwrap_or_else(|| rustified_core::session::music_url(index))
        };
        if let Some(webview) = &self.webview {
            let _ = webview.load_url(&url);
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
                self.recovered.set(false);
                self.requested.set(Some(Instant::now()));
                let play = json!({"op":"play","generation":generation,"id":video_id,"position":0,"paused":false});
                *self.resume.borrow_mut() = Some(play.clone());
                self.restoring.set(false);
                self.waiting_restore.set(false);
                if self.prefer_direct.get() && !self.login_open.get() {
                    self.recovered.set(true);
                    self.reconnect();
                    return;
                }
                play
            }
            PlayerCommand::Toggle => {
                if let Some(resume) = self.resume.borrow_mut().as_mut() {
                    resume["paused"] = json!(!resume["paused"].as_bool().unwrap_or(false));
                }
                json!({"op":"toggle"})
            }
            PlayerCommand::Stop => {
                self.requested.set(None);
                if let Some(resume) = self.resume.borrow_mut().as_mut() {
                    resume["paused"] = json!(true);
                }
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
        if self.ready.get() && self.allowed() {
            self.eval(&message);
        } else {
            self.restoring.set(true);
            self.requested.set(None);
        }
    }
    pub fn seek(&self, seconds: f64) -> u64 {
        let revision = self.seek_revision.get() + 1;
        self.seek_revision.set(revision);
        if let Some(resume) = self.resume.borrow_mut().as_mut() {
            resume["position"] = json!(seconds.max(0.));
            resume["revision"] = json!(revision);
        }
        let message = json!({"op":"seek","value":seconds.max(0.0),"revision":revision});
        if self.ready.get() && self.allowed() {
            self.eval(&message);
        } else {
            self.restoring.set(true);
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
        for event in [
            r#"{"type":"popup","url":"https://accounts.google.com/"}"#,
            r#"{"type":"navigation","destination":"account"}"#,
            r#"{"type":"captured","capture":{"id":0,"result":null}}"#,
        ] {
            assert!(serde_json::from_str::<super::BridgeEvent>(event).is_err());
        }

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
