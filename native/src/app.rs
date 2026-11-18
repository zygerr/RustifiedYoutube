use crate::{
    backend::{Backend, Request, Response},
    icons::{self, Icon},
    player::{Player, PlayerCommand, PlayerState},
};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Vec2};
use rustified_core::model::{Playlist, SearchFilter, Track, format_time};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::mpsc,
    time::Duration,
};

use crate::{
    catalog_ui as catalog,
    theme::{self, Theme},
};

#[derive(Clone, PartialEq)]
enum Page {
    Discover,
    Library,
    Tracks(String),
    Queue,
    Settings,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    theme: Theme,
    volume: Option<f32>,
    recent: Vec<Track>,
    close_to_tray: Option<bool>,
    volume_scroll_step: Option<u8>,
    shuffle: Option<bool>,
    discord_rpc: Option<bool>,
    discord_application_id: Option<String>,
}

struct StreamCheck {
    started: std::time::Instant,
    selected: Option<std::time::Instant>,
    stage: u8,
    last_trace: u64,
    session_refreshed: bool,
    saw_ad: bool,
    taskbar_checked: bool,
}
struct AccountCheck {
    started: std::time::Instant,
    phase: u8,
}

enum TrayAction {
    Show,
    Toggle,
    Next,
    Previous,
    Quit,
}

pub struct MusicApp {
    backend: Backend,
    player: Player,
    discord: crate::discord::Discord,
    taskbar: Option<crate::taskbar::Taskbar>,
    state: PlayerState,
    page: Page,
    query: String,
    search_filter: SearchFilter,
    tracks: Vec<Track>,
    discover_tracks: Vec<Track>,
    discover_loading: bool,
    discover_error: Option<String>,
    playlists: Vec<Playlist>,
    queue: Vec<Track>,
    queue_original: Vec<Track>,
    account_recent: Vec<Track>,
    session_recent: Vec<Track>,
    history_loading: bool,
    history_error: Option<String>,
    recorded_generation: u64,
    current: Option<usize>,
    now: Option<Track>,
    loading: bool,
    resolving: bool,
    preparation: String,
    connecting: bool,
    connected: bool,
    account_status: String,
    library_loading: bool,
    library_loaded: bool,
    library_error: Option<String>,
    account_retry_at: Option<std::time::Instant>,
    ready: bool,
    error: Option<String>,
    list_request: u64,
    play_request: u64,
    volume: f32,
    volume_wheel: crate::volume::Wheel,
    artwork_wheel: crate::volume::Wheel,
    thumbnail_wheel: crate::volume::Wheel,
    repeat: bool,
    shuffle: bool,
    preferences: Preferences,
    textures: HashMap<String, egui::TextureHandle>,
    texture_order: VecDeque<String>,
    requested: HashSet<String>,
    tray: Option<tray_icon::TrayIcon>,
    tray_rx: mpsc::Receiver<TrayAction>,
    quit: bool,
    window_guard: crate::window::Guard,
    about: bool,
    timeline: crate::timeline::Timeline,
    expanded: bool,
    video_mode: bool,
    video_rect: Option<egui::Rect>,
    stream_check: Option<StreamCheck>,
    snapshot: Option<crate::preview::Snapshot>,
    account_check: Option<AccountCheck>,
}

impl MusicApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        taskbar_events: Option<crate::taskbar::Events>,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        crate::fonts::install(ctx);
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(14., 9.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.));
        ctx.set_style(style);
        let preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        crate::preview::theme()
            .unwrap_or(preferences.theme)
            .apply(ctx);
        let volume = preferences.volume.unwrap_or(0.5);
        let shuffle = preferences.shuffle.unwrap_or(false);
        let (tray_tx, tray_rx) = mpsc::channel();
        let tray = Self::create_tray(ctx.clone(), tray_tx).ok();
        let player = Player::start(cc, volume as f64);
        let taskbar = crate::taskbar::Taskbar::start(cc, taskbar_events);
        player.send(PlayerCommand::Volume(volume as f64));
        let mut app = Self {
            backend: Backend::start(ctx.clone()),
            player,
            taskbar,
            discord: crate::discord::Discord::start(ctx.clone()),
            state: PlayerState::default(),
            page: Page::Discover,
            query: String::new(),
            search_filter: SearchFilter::All,
            tracks: Vec::new(),
            discover_tracks: Vec::new(),
            discover_loading: true,
            discover_error: None,
            playlists: Vec::new(),
            queue: Vec::new(),
            queue_original: Vec::new(),
            account_recent: Vec::new(),
            session_recent: Vec::new(),
            history_loading: false,
            history_error: None,
            recorded_generation: 0,
            current: None,
            now: None,
            loading: true,
            resolving: false,
            preparation: String::new(),
            connecting: false,
            connected: false,
            account_status: "Restoring the saved Google session…".into(),
            library_loading: false,
            library_loaded: false,
            library_error: None,
            account_retry_at: None,
            ready: false,
            error: None,
            list_request: 0,
            play_request: 0,
            volume,
            volume_wheel: crate::volume::Wheel::default(),
            artwork_wheel: Default::default(),
            thumbnail_wheel: Default::default(),
            repeat: false,
            shuffle,
            preferences,
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            requested: HashSet::new(),
            tray,
            tray_rx,
            quit: false,
            window_guard: crate::window::Guard::new(cc.storage),
            about: false,
            timeline: crate::timeline::Timeline::default(),
            expanded: false,
            video_mode: false,
            video_rect: None,
            stream_check: std::env::args()
                .any(|arg| arg == "--smoke-stream")
                .then(|| StreamCheck {
                    started: std::time::Instant::now(),
                    selected: None,
                    stage: 0,
                    last_trace: 0,
                    session_refreshed: false,
                    saw_ad: false,
                    taskbar_checked: false,
                }),
            snapshot: crate::preview::Snapshot::new(),
            account_check: crate::account::smoke_active().then(|| AccountCheck {
                started: std::time::Instant::now(),
                phase: if std::env::args().any(|arg| arg == "--smoke-sign-out") {
                    20
                } else if std::env::args().any(|arg| arg == "--smoke-login") {
                    10
                } else {
                    0
                },
            }),
        };
        app.prepare_snapshot(ctx);
        app
    }

    fn prepare_snapshot(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        self.preferences.theme = crate::preview::theme().unwrap_or_default();
        self.page = match snapshot.page.as_str() {
            "library" => Page::Library,
            "search" => Page::Tracks("Search results".into()),
            "queue" => Page::Queue,
            "settings" => Page::Settings,
            _ => Page::Discover,
        };
        self.expanded = snapshot.page == "player";
        self.tracks = crate::preview::tracks();
        self.discover_tracks = self.tracks.clone();
        self.account_recent = self.tracks.clone();
        self.session_recent.clear();
        self.preferences.recent.clear();
        self.query = if snapshot.page == "search" {
            "nova lane".into()
        } else {
            String::new()
        };
        self.queue = self.tracks.clone();
        self.queue_original = self.tracks.clone();
        self.current = Some(0);
        self.now = self.tracks.first().cloned();
        self.playlists = crate::preview::playlists();
        self.connected = true;
        self.account_status = "Google account connected · preview".into();
        self.ready = true;
        self.loading = false;
        self.discover_loading = false;
        self.history_loading = false;
        self.volume = 0.42;
        self.state = PlayerState {
            playing: true,
            position: 72.,
            duration: 208.,
            ..Default::default()
        };
        self.textures = crate::preview::textures(ctx).into_iter().collect();
    }

    fn create_tray(
        ctx: egui::Context,
        tx: mpsc::Sender<TrayAction>,
    ) -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
        use muda::{Menu, MenuItem, PredefinedMenuItem};
        let menu = Menu::new();
        let show = MenuItem::with_id("show", "Open RustifiedYoutube", true, None);
        let play = MenuItem::with_id("play", "Play / pause", true, None);
        let next = MenuItem::with_id("next", "Next track", true, None);
        let previous = MenuItem::with_id("previous", "Previous track", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        menu.append_items(&[
            &show,
            &play,
            &next,
            &previous,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let event_ctx = ctx.clone();
        let event_tx = tx.clone();
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            let action = match event.id.0.as_str() {
                "play" => TrayAction::Toggle,
                "next" => TrayAction::Next,
                "previous" => TrayAction::Previous,
                "quit" => TrayAction::Quit,
                _ => TrayAction::Show,
            };
            let _ = event_tx.send(action);
            event_ctx.request_repaint();
        }));
        tray_icon::TrayIconEvent::set_event_handler(Some(
            move |event: tray_icon::TrayIconEvent| {
                if matches!(
                    event,
                    tray_icon::TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    }
                ) {
                    let _ = tx.send(TrayAction::Show);
                    ctx.request_repaint();
                }
            },
        ));
        let image = image::load_from_memory(include_bytes!("../../assets/icon.png"))?.to_rgba8();
        let icon =
            tray_icon::Icon::from_rgba(image.clone().into_raw(), image.width(), image.height())?;
        Ok(tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip(format!("RustifiedYoutube v{}", env!("CARGO_PKG_VERSION")))
            .with_menu_on_left_click(false)
            .build()?)
    }

    fn search(&mut self, query: String) {
        if query.trim().is_empty() {
            return;
        }
        self.list_request += 1;
        self.expanded = false;
        self.loading = true;
        self.error = None;
        self.page = Page::Tracks(format!("Results for “{}”", query.trim()));
        self.backend.send(Request::Search(
            self.list_request,
            query,
            self.search_filter,
        ));
    }

    fn playlist(&mut self, id: String, title: String) {
        self.expanded = false;
        self.list_request += 1;
        self.loading = true;
        self.error = None;
        self.page = Page::Tracks(title);
        self.backend.send(Request::Playlist(self.list_request, id));
    }

    fn play_at(&mut self, index: usize) {
        let Some(track) = self.queue.get(index).cloned() else {
            return;
        };
        self.current = Some(index);
        self.now = Some(track.clone());
        self.play_request += 1;
        if let Some(check) = &mut self.stream_check {
            check.selected = Some(std::time::Instant::now());
        }
        self.resolving = false;
        self.preparation = "Starting YouTube stream…".into();
        self.timeline.reset();
        self.state = PlayerState::default();
        self.error = None;
        self.player.send(PlayerCommand::Stop);
        self.state.buffering = true;
        self.player.send(PlayerCommand::Play {
            generation: self.play_request,
            video_id: track.id.clone(),
        });
    }

    fn start_queue(&mut self, tracks: Vec<Track>, index: usize) {
        if index >= tracks.len() {
            return;
        }
        self.queue_original = tracks.clone();
        self.queue = tracks;
        let index = if self.shuffle {
            crate::queue::shuffle_from(&mut self.queue, index, &mut rand::rng())
        } else {
            index
        };
        self.play_at(index);
    }
    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.preferences.shuffle = Some(self.shuffle);
        if self.shuffle {
            crate::queue::shuffle_remaining(&mut self.queue, self.current, &mut rand::rng());
        } else {
            crate::queue::restore_remaining(&mut self.queue, &self.queue_original, self.current);
        }
    }
    fn seek_to(&mut self, seconds: f64) {
        let target = seconds.clamp(0.0, self.state.duration.max(0.0));
        let revision = self.player.seek(target);
        self.timeline.request(target, revision, self.play_request);
    }

    fn next(&mut self) {
        if let Some(index) = self.current {
            if index + 1 < self.queue.len() {
                self.play_at(index + 1);
            } else if self.repeat && !self.queue.is_empty() {
                if self.shuffle && self.queue.len() > 1 {
                    crate::queue::shuffle_remaining(&mut self.queue, None, &mut rand::rng());
                    if self
                        .now
                        .as_ref()
                        .is_some_and(|now| now.id == self.queue[0].id)
                        && let Some(other) = self
                            .queue
                            .iter()
                            .position(|track| track.id != self.queue[0].id)
                    {
                        self.queue.swap(0, other);
                    }
                }
                self.play_at(0);
            }
        }
    }

    fn previous(&mut self) {
        if self.state.position > 3. {
            self.seek_to(0.);
        } else if let Some(index) = self.current {
            self.play_at(index.saturating_sub(1));
        }
    }

    fn clear_account_data(&mut self) {
        self.playlists.clear();
        self.account_recent.clear();
        self.library_loading = false;
        self.library_loaded = false;
        self.library_error = None;
        self.history_loading = false;
        self.history_error = None;
        self.account_retry_at = None;
    }

    fn request_library(&mut self) {
        if self.connected && !self.library_loading {
            self.library_loading = true;
            self.backend.send(Request::Library);
        }
    }

    fn request_history(&mut self) {
        if self.connected && !self.history_loading {
            self.history_loading = true;
            self.backend.send(Request::History);
        }
    }

    fn retry_account_data(&mut self, ctx: &egui::Context) {
        let Some(retry_at) = self.account_retry_at else {
            return;
        };
        if !self.connected {
            self.account_retry_at = None;
            return;
        }
        let remaining = retry_at.saturating_duration_since(std::time::Instant::now());
        if !remaining.is_zero() {
            ctx.request_repaint_after(remaining);
        } else if self.connecting || self.player.login_open() || self.player.needs_login() {
            ctx.request_repaint_after(Duration::from_secs(15));
        } else {
            self.account_retry_at = None;
            if self.library_error.is_some() {
                self.request_library();
            }
            if self.history_error.is_some() {
                self.request_history();
            }
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.player.pump();
        if let Some(result) = self.player.take_google_sign_out_result() {
            match result {
                Ok(()) => self.backend.send(Request::FinishGoogleSignOut),
                Err(message) => {
                    self.player.cancel_google_sign_out();
                    self.backend.send(Request::CancelGoogleSignOut(message));
                }
            }
        }
        if self.player.take_login_completed() {
            self.error = None;
            self.connecting = rustified_core::session::native_enabled();
            if self.connecting {
                self.backend.send(Request::Connect { force_login: false });
            }
        }
        if self.player.take_login_failed() {
            self.connecting = false;
        }
        while let Ok(capture) = self.player.session_rx.try_recv() {
            self.backend.send(Request::Captured(capture));
        }
        if self.player.take_quit() {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        match self.player.take_media_action() {
            Some(crate::player::MediaAction::Next) => self.next(),
            Some(crate::player::MediaAction::Previous) => self.previous(),
            None => {}
        }
        while let Ok(response) = self.backend.rx.try_recv() {
            match response {
                Response::Ready(connected) => {
                    self.connected = connected;
                    self.player.require_account(
                        connected
                            || (rustified_core::session::native_enabled()
                                && rustified_core::session::load_session()
                                    .ok()
                                    .flatten()
                                    .is_some()),
                    );
                    self.ready = true;
                    if connected {
                        self.request_library();
                        self.request_history();
                    } else {
                        self.clear_account_data();
                    }
                    if self.discover_tracks.is_empty() {
                        self.discover_loading = true;
                        self.backend.send(Request::Discover);
                    }
                }
                Response::CaptureSession(id) => self.player.capture_session(id),
                Response::AccountStatus(status) => self.account_status = status,
                Response::OpenSignIn => {
                    if crate::account::smoke_active()
                        && !std::env::args().any(|arg| arg == "--smoke-login")
                    {
                        crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                        self.quit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.error = None;
                        self.player.begin_login();
                    }
                }
                Response::StartGoogleSignOut => {
                    if let Err(message) = self.player.begin_google_sign_out() {
                        self.player.cancel_google_sign_out();
                        self.backend.send(Request::CancelGoogleSignOut(message));
                    }
                }
                Response::GoogleSignOutFinished(success) => {
                    self.connecting = false;
                    if std::env::args().any(|arg| arg == "--smoke-sign-out")
                        && let Some(check) = &mut self.account_check
                    {
                        check.phase = if success { 23 } else { 24 };
                    }
                    if success {
                        self.connected = false;
                        // A library or history request still in flight is
                        // dropped once disconnected; reset its loading flag so
                        // the next connection can request the data again.
                        self.clear_account_data();
                        self.player.require_account(false);
                    }
                }
                Response::ConnectFinished => {
                    if !self.player.login_open() {
                        self.connecting = false;
                    }
                }
                Response::Tracks(id, result) if id == self.list_request => {
                    self.loading = false;
                    match result {
                        Ok(tracks) => self.tracks = tracks,
                        Err(message) => self.error = Some(message),
                    }
                }
                Response::History(result) if self.connected => {
                    self.history_loading = false;
                    match result {
                        Ok(tracks) => {
                            self.account_recent = tracks;
                            self.history_error = None;
                        }
                        Err(message) => {
                            self.history_error = Some(message);
                            self.account_retry_at.get_or_insert_with(|| {
                                std::time::Instant::now() + Duration::from_secs(15)
                            });
                        }
                    }
                }
                Response::Discover(result) => {
                    self.discover_loading = false;
                    match result {
                        Ok(tracks) => {
                            self.discover_tracks = tracks;
                            self.discover_error = None;
                        }
                        Err(message) => self.discover_error = Some(message),
                    }
                }
                Response::Library(result) if self.connected => {
                    self.library_loading = false;
                    match result {
                        Ok(playlists) => {
                            self.playlists = playlists;
                            self.library_loaded = true;
                            self.library_error = None;
                        }
                        Err(message) => {
                            self.library_error = Some(message);
                            self.account_retry_at.get_or_insert_with(|| {
                                std::time::Instant::now() + Duration::from_secs(15)
                            });
                        }
                    }
                }
                Response::Liked(id, liked, success) => {
                    if success {
                        for track in self
                            .tracks
                            .iter_mut()
                            .chain(self.discover_tracks.iter_mut())
                            .chain(self.queue.iter_mut())
                            .chain(self.queue_original.iter_mut())
                            .chain(self.account_recent.iter_mut())
                            .chain(self.session_recent.iter_mut())
                            .chain(self.preferences.recent.iter_mut())
                        {
                            if track.id == id {
                                track.liked = liked;
                            }
                        }
                        if let Some(track) = &mut self.now
                            && track.id == id
                        {
                            track.liked = liked;
                        }
                    } else {
                        self.error = Some(
                            "Couldn't update liked songs. Try again after the connection recovers."
                                .into(),
                        );
                    }
                }
                Response::Cover(url, image) => {
                    // A re-fetched cover replaces its old texture; drop the stale
                    // eviction entry so it cannot evict the new texture early.
                    if self.textures.remove(&url).is_some() {
                        self.texture_order.retain(|old| old != &url);
                    }
                    let bytes = image.width() * image.height() * 4;
                    let mut used = self
                        .textures
                        .values()
                        .map(|texture| texture.size()[0] * texture.size()[1] * 4)
                        .sum::<usize>();
                    while self.textures.len() >= 128 || used + bytes > crate::artwork::CACHE_BYTES {
                        let Some(old) = self.texture_order.pop_front() else {
                            break;
                        };
                        if let Some(texture) = self.textures.remove(&old) {
                            used = used.saturating_sub(texture.size()[0] * texture.size()[1] * 4);
                        }
                        self.requested.remove(&old);
                    }
                    self.textures.insert(
                        url.clone(),
                        ctx.load_texture(&url, image, egui::TextureOptions::LINEAR),
                    );
                    self.texture_order.push_back(url);
                }
                Response::Notice(message) => {
                    self.connecting = false;
                    self.loading = false;
                    self.discover_loading = false;
                    self.error = Some(message);
                }
                _ => {}
            }
        }
        while let Ok(state) = self.player.rx.try_recv() {
            if self.resolving || (state.generation != self.play_request && state.generation != 0) {
                continue;
            }
            if let Some(check) = &mut self.stream_check {
                check.saw_ad |= state.ad;
            }
            let ended = state.ended;
            self.timeline.observe(&state);
            if state.playing
                && !state.ad
                && state.position > 0.5
                && self.recorded_generation != self.play_request
                && self.stream_check.is_none()
            {
                self.recorded_generation = self.play_request;
                if let Some(track) = &self.now {
                    for recent in [&mut self.preferences.recent, &mut self.session_recent] {
                        recent.retain(|item| item.id != track.id);
                        recent.insert(0, track.clone());
                        recent.truncate(30);
                    }
                }
            }
            if state.playing && self.state.error.is_some() {
                self.error = None;
            }
            if let Some(error) = &state.error {
                self.error = Some(error.clone());
            }
            self.state = state;
            if ended {
                self.next();
            }
        }
        if self.timeline.expire() {
            self.error =
                Some("YouTube couldn't confirm the seek position. Try seeking again.".into());
        }
        if self.player.needs_login() || self.player.login_open() {
            self.state.playing = false;
            self.state.buffering = false;
        }
        let taskbar_actions = self
            .taskbar
            .as_mut()
            .map(|taskbar| taskbar.poll())
            .unwrap_or_default();
        for action in taskbar_actions {
            match action {
                crate::taskbar::Action::Toggle => self.player.send(PlayerCommand::Toggle),
                crate::taskbar::Action::Previous => self.previous(),
                crate::taskbar::Action::Next => self.next(),
            }
            if crate::taskbar::smoke_active() {
                println!("Taskbar action dispatched: {action:?}");
            }
        }
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayAction::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayAction::Toggle => self.player.send(PlayerCommand::Toggle),
                TrayAction::Next => self.next(),
                TrayAction::Previous => self.previous(),
                TrayAction::Quit => {
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn artwork(&mut self, url: &str) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.textures.get(url) {
            return Some(texture.clone());
        }
        if !url.is_empty() && self.requested.insert(url.into()) {
            self.backend.send(Request::Cover(url.into(), false));
        }
        None
    }
    fn cover(&mut self, ui: &mut egui::Ui, url: &str, size: f32) -> egui::Response {
        let texture = if size > catalog::CARD_ART {
            let key = crate::artwork::key(url, true);
            if let Some(texture) = self.textures.get(&key) {
                Some(texture.clone())
            } else {
                if !url.is_empty() && self.requested.insert(key) {
                    self.backend.send(Request::Cover(url.into(), true));
                }
                self.artwork(url)
            }
        } else {
            self.artwork(url)
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        catalog::cover(ui, rect, texture.as_ref());
        response
    }
    fn artwork_volume(&mut self, ui: &mut egui::Ui, response: egui::Response, large: bool) {
        let colors = crate::theme::palette(ui.ctx());
        let rect = response.rect;
        let step = self.preferences.volume_scroll_step.unwrap_or(5);
        let wheel = if large {
            &mut self.artwork_wheel
        } else {
            &mut self.thumbnail_wheel
        };
        if wheel.over(ui, rect, &mut self.volume, step) {
            self.player.send(PlayerCommand::Volume(self.volume as f64));
            ui.ctx().request_repaint();
        }
        if large && response.hovered() {
            let badge = egui::Rect::from_min_size(
                rect.right_bottom() - Vec2::new(80., 42.),
                Vec2::new(68., 30.),
            );
            ui.painter()
                .rect_filled(badge, 6., Color32::from_black_alpha(185));
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}%", self.volume * 100.),
                FontId::proportional(14.),
                colors.text,
            );
        }
        response.on_hover_text(format!(
            "Volume: {:.0}% · Scroll ±{}%",
            self.volume * 100.,
            step
        ));
    }

    fn nav(ui: &mut egui::Ui, text: &str, icon: Icon, selected: bool) -> bool {
        let colors = crate::theme::palette(ui.ctx());
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                8.,
                if selected {
                    colors.selected
                } else {
                    colors.hover
                },
            );
        }
        let color = colors.text;
        icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(rect.left_center() + Vec2::new(20., 0.), Vec2::splat(22.)),
            icon,
            color,
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(43., 0.),
            egui::Align2::LEFT_CENTER,
            text,
            FontId::proportional(15.),
            color,
        );
        response.clicked()
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let colors = crate::theme::palette(ctx);
        egui::SidePanel::left("navigation")
            .exact_width(224.)
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .stroke(egui::Stroke::new(1_f32, colors.border))
                    .inner_margin(12),
            )
            .show(ctx, |ui| {
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                    ui.painter().circle_stroke(
                        rect.center(),
                        12.,
                        egui::Stroke::new(2_f32, colors.progress),
                    );
                    icons::draw(ui.painter(), rect.shrink(6.), Icon::Play, colors.progress);
                    ui.add(
                        egui::Label::new(RichText::new("RustifiedYoutube").size(17.).strong())
                            .truncate(),
                    );
                });
                ui.add_space(24.);
                if Self::nav(ui, "Discover", Icon::Home, self.page == Page::Discover) {
                    self.page = Page::Discover;
                    self.expanded = false;
                }
                if Self::nav(
                    ui,
                    "Your library",
                    Icon::Library,
                    self.page == Page::Library,
                ) {
                    self.page = Page::Library;
                    self.expanded = false;
                    self.request_library();
                }
                if Self::nav(
                    ui,
                    "Liked songs",
                    Icon::Heart,
                    matches!(&self.page,Page::Tracks(t) if t=="Liked songs"),
                ) {
                    self.playlist("LM".into(), "Liked songs".into());
                }
                if Self::nav(ui, "Play queue", Icon::Queue, self.page == Page::Queue) {
                    self.page = Page::Queue;
                    self.expanded = false;
                }
                ui.add_space(16.);
                ui.separator();
                ui.add_space(14.);
                ui.label(
                    RichText::new("YOUR PLAYLISTS")
                        .size(10.)
                        .color(colors.muted)
                        .strong(),
                );
                ui.add_space(8.);
                let playlists = self.playlists.clone();
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_playlists")
                    .max_height((ui.available_height() - 150.).max(60.))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        for playlist in playlists {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 44.), Sense::click());
                            let active = matches!(&self.page, Page::Tracks(title) if title == &playlist.title);
                            if active || response.hovered() {
                                ui.painter().rect_filled(rect, 6., if active { colors.selected } else { colors.hover });
                            }
                            for (text, size, y, color) in [
                                (playlist.title.as_str(), 13., 6., colors.text),
                                ("Playlist", 11., 25., colors.muted),
                            ] {
                                let mut job = egui::text::LayoutJob::simple(text.into(), FontId::proportional(size), color, rect.width() - 20.);
                                job.wrap.max_rows = 1;
                                let galley = ui.painter().layout_job(job);
                                ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).galley(rect.min + Vec2::new(10., y), galley, color);
                            }
                            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &playlist.title));
                            if response.on_hover_text(&playlist.title).clicked() {
                                self.playlist(playlist.id, playlist.title);
                            }
                        }
                        if self.playlists.is_empty() {
                            ui.label(
                                RichText::new(if self.connected {
                                    "Your playlists will appear here."
                                } else {
                                    "Connect to bring your playlists."
                                })
                                .size(12.)
                                .color(colors.muted),
                            );
                        }
                    });
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("v{}  ·  About", env!("CARGO_PKG_VERSION")))
                                    .size(11.)
                                    .color(colors.muted),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.about = true;
                    }
                    if Self::nav(ui, "Settings", Icon::Settings, self.page == Page::Settings) {
                        self.page = Page::Settings;
                        self.expanded = false;
                    }
                    ui.add_space(10.);
                    if self.connected && !self.player.needs_login() {
                        ui.label(
                            RichText::new(if self.player.authenticated() == Some(true) {
                                "●  Player connected"
                            } else {
                                "●  Checking playback session…"
                            })
                            .size(11.)
                            .color(colors.accent),
                        )
                        .on_hover_text(&self.account_status);
                    } else if ui
                        .add_enabled(
                            !self.connecting,
                            egui::Button::new(if self.connecting {
                                "Connecting…"
                            } else {
                                "Continue with Google"
                            }),
                        )
                        .clicked()
                    {
                        self.connecting = true;
                        self.backend.send(Request::Connect {
                            force_login: self.player.authenticated() != Some(true),
                        });
                    }
                });
            });
    }

    fn player_bar(&mut self, ctx: &egui::Context) {
        let colors = crate::theme::palette(ctx);
        egui::TopBottomPanel::bottom("player")
            .exact_height(96.)
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .stroke(egui::Stroke::new(1_f32, colors.border))
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 52.),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let now_card = ui.allocate_ui_with_layout(
                            Vec2::new((ui.available_width() * 0.31).max(230.), 52.),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                if let Some(track) = self.now.clone() {
                                    let cover = self.cover(ui, &track.cover, catalog::ROW_ART);
                                    self.artwork_volume(ui, cover, false);
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(&track.title).strong())
                                                .truncate(),
                                        )
                                        .on_hover_text(&track.title);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&track.artist)
                                                    .size(12.)
                                                    .color(colors.muted),
                                            )
                                            .truncate(),
                                        );
                                    });
                                } else {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::splat(catalog::ROW_ART),
                                        Sense::hover(),
                                    );
                                    catalog::cover(ui, rect, None);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("Nothing playing").strong());
                                        ui.label(
                                            RichText::new("Choose a song to start")
                                                .size(12.)
                                                .color(colors.muted),
                                        );
                                    });
                                }
                            },
                        );
                        let expand = now_card
                            .response
                            .interact(Sense::click())
                            .on_hover_text("Open expanded player");
                        expand.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Open expanded player",
                            )
                        });
                        if expand.clicked() && self.now.is_some() {
                            self.expanded = !self.expanded;
                        }
                        ui.allocate_ui_with_layout(
                            Vec2::new((ui.available_width() - 240.).max(244.), 52.),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.horizontal_centered(|ui| {
                                    ui.add_space(((ui.available_width() - 214.) / 2.).max(0.));
                                    let shuffle = icons::button(
                                        ui,
                                        Icon::Shuffle,
                                        "Shuffle upcoming tracks",
                                        30.,
                                        false,
                                    );
                                    if shuffle.clicked() {
                                        self.toggle_shuffle();
                                    }
                                    if self.shuffle {
                                        ui.painter().circle_filled(
                                            shuffle.rect.center_bottom() + Vec2::new(0., 2.),
                                            2.,
                                            colors.accent,
                                        );
                                    }
                                    if icons::button(
                                        ui,
                                        Icon::Previous,
                                        "Previous track",
                                        36.,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.previous();
                                    }
                                    if self.resolving || self.state.buffering {
                                        ui.add_sized([42., 42.], egui::Spinner::new());
                                    } else if icons::button(
                                        ui,
                                        if self.state.playing {
                                            Icon::Pause
                                        } else {
                                            Icon::Play
                                        },
                                        "Play / pause",
                                        42.,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        if self.now.is_some() {
                                            self.player.send(PlayerCommand::Toggle);
                                        } else {
                                            let tracks = if self.page == Page::Discover {
                                                self.discover_tracks.clone()
                                            } else {
                                                self.tracks.clone()
                                            };
                                            self.start_queue(tracks, 0);
                                        }
                                    }
                                    if icons::button(ui, Icon::Next, "Next track", 36., false)
                                        .clicked()
                                    {
                                        self.next();
                                    }
                                    let response =
                                        icons::button(ui, Icon::Repeat, "Repeat queue", 30., false);
                                    if response.clicked() {
                                        self.repeat = !self.repeat;
                                    }
                                    if self.repeat {
                                        ui.painter().circle_filled(
                                            response.rect.center_bottom() + Vec2::new(0., 2.),
                                            2.,
                                            colors.accent,
                                        );
                                    }
                                });
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icons::button(ui, Icon::Queue, "Open play queue", 32., false)
                                .clicked()
                            {
                                self.page = Page::Queue;
                                self.expanded = false;
                            }
                            if self.volume_wheel.control(
                                ui,
                                &mut self.volume,
                                self.preferences.volume_scroll_step.unwrap_or(5),
                            ) {
                                self.player.send(PlayerCommand::Volume(self.volume as f64));
                            }
                            let (volume_icon, _) =
                                ui.allocate_exact_size(Vec2::splat(24.), Sense::hover());
                            icons::draw(ui.painter(), volume_icon, Icon::Volume, colors.muted);
                        });
                    },
                );
                self.timeline_ui(ui, "compact_timeline");
            });
    }

    fn timeline_ui(&mut self, ui: &mut egui::Ui, id: &str) {
        let colors = crate::theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            let clock_width = 48.;
            ui.add_sized(
                [clock_width, 24.],
                egui::Label::new(
                    RichText::new(format_time(self.timeline.position(self.state.position)))
                        .size(11.)
                        .color(colors.muted),
                ),
            );
            let width = (ui.available_width() - clock_width - ui.spacing().item_spacing.x).max(1.);
            let seek = ui
                .allocate_ui_with_layout(
                    Vec2::new(width, 24.),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        self.timeline
                            .show(
                                ui,
                                id,
                                self.state.duration,
                                self.state.position,
                                self.now.is_some() && !self.state.ad,
                            )
                            .1
                    },
                )
                .inner;
            ui.add_sized(
                [clock_width, 24.],
                egui::Label::new(
                    RichText::new(format_time(self.state.duration))
                        .size(11.)
                        .color(colors.muted),
                ),
            );
            if let Some(target) = seek {
                self.seek_to(target);
                ui.ctx().request_repaint();
            }
        });
    }

    fn recent_tracks(&self) -> Vec<Track> {
        let mut seen = HashSet::new();
        self.session_recent
            .iter()
            .chain(&self.account_recent)
            .chain(&self.preferences.recent)
            .filter(|track| seen.insert(track.id.clone()))
            .take(30)
            .cloned()
            .collect()
    }

    fn media_card(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        title: &str,
        subtitle: &str,
        url: &str,
        action: &str,
    ) -> egui::Response {
        let visible = ui.is_rect_visible(egui::Rect::from_min_size(
            ui.next_widget_position(),
            Vec2::new(catalog::CARD_ART, catalog::CARD_HEIGHT),
        ));
        let texture = if visible { self.artwork(url) } else { None };
        catalog::card(ui, id, title, subtitle, texture.as_ref(), action)
    }

    fn song_shelf(&mut self, ui: &mut egui::Ui, id: &str, tracks: &[Track]) {
        egui::ScrollArea::horizontal()
            .id_salt(id)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = catalog::CARD_GAP;
                ui.horizontal_top(|ui| {
                    for (index, track) in tracks.iter().enumerate() {
                        if self
                            .media_card(
                                ui,
                                ui.id().with((id, index)),
                                &track.title,
                                &track.artist,
                                &track.cover,
                                &format!("Play {}", track.title),
                            )
                            .clicked()
                        {
                            self.start_queue(tracks.to_vec(), index);
                        }
                    }
                });
            });
    }

    fn recent_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("YOUR MUSIC").size(11.).color(colors.muted));
                ui.label(RichText::new("Listen again").size(30.).strong());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.connected
                    && ui
                        .add_enabled(
                            !self.history_loading,
                            egui::Button::new("Refresh").corner_radius(18),
                        )
                        .clicked()
                {
                    self.request_history();
                }
                if self.history_loading {
                    ui.spinner();
                }
            });
        });
        ui.add_space(16.);
        if let Some(message) = &self.history_error {
            ui.label(RichText::new(message).small().color(colors.muted));
        }
        let recent = self.recent_tracks();
        if recent.is_empty() && self.history_loading {
            catalog::skeleton_shelf(ui);
        } else if recent.is_empty() {
            ui.label(
                RichText::new("Your recently played songs will appear here.").color(colors.muted),
            );
        } else {
            self.song_shelf(ui, "recent_tracks", &recent);
        }
    }

    fn song_row(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        track: &Track,
        actions: bool,
    ) -> Option<catalog::SongAction> {
        let visible = ui.is_rect_visible(egui::Rect::from_min_size(
            ui.next_widget_position(),
            Vec2::new(ui.available_width(), catalog::ROW_HEIGHT),
        ));
        let texture = if visible {
            self.artwork(&track.cover)
        } else {
            None
        };
        let active = self.now.as_ref().is_some_and(|now| now.id == track.id);
        catalog::song_row(ui, id, track, texture.as_ref(), active, actions)
    }

    fn song_action(&mut self, track: &Track, action: catalog::SongAction) {
        match action {
            catalog::SongAction::Add => {
                self.queue.push(track.clone());
                self.queue_original.push(track.clone());
            }
            catalog::SongAction::Like => {
                if self.connected {
                    self.backend
                        .send(Request::Like(track.id.clone(), !track.liked));
                } else {
                    self.error = Some("Connect your Google account to like songs.".into());
                }
            }
            catalog::SongAction::Play => {}
        }
    }

    fn discover_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        egui::ScrollArea::vertical()
            .id_salt("discover_page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.;
                    for (label, query) in [
                        ("Relax", "chill acoustic"),
                        ("Energize", "energetic music"),
                        ("Focus", "lofi focus"),
                        ("Workout", "workout music"),
                        ("Commute", "driving music"),
                        ("Romanian", "Romanian music"),
                    ] {
                        if ui
                            .add(
                                egui::Button::new(label)
                                    .corner_radius(8)
                                    .min_size(Vec2::new(76., 34.)),
                            )
                            .clicked()
                        {
                            self.query = query.into();
                            self.search(query.into());
                        }
                    }
                });
                ui.add_space(28.);
                self.recent_ui(ui);
                ui.add_space(32.);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("START WITH A SONG")
                                .size(11.)
                                .color(colors.muted),
                        );
                        ui.label(RichText::new("Quick picks").size(30.).strong());
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !self.discover_tracks.is_empty(),
                                egui::Button::new("▶  Play all").corner_radius(20),
                            )
                            .clicked()
                        {
                            self.start_queue(self.discover_tracks.clone(), 0);
                        }
                    });
                });
                ui.add_space(16.);
                if self.discover_loading {
                    catalog::skeleton_rows(ui, 4);
                } else if let Some(message) = self.discover_error.clone() {
                    ui.label(RichText::new(message).color(colors.muted));
                    if ui.button("Try again").clicked() {
                        self.discover_loading = true;
                        self.backend.send(Request::Discover);
                    }
                } else {
                    let tracks = self.discover_tracks.clone();
                    let picks = &tracks[..tracks.len().min(12)];
                    let columns = catalog::columns(ui.available_width(), 300., 24.).min(3);
                    let width =
                        (ui.available_width() - 24. * (columns - 1) as f32) / columns as f32;
                    let rows = picks.len().div_ceil(columns).max(1);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 24.;
                        for column in 0..columns {
                            ui.allocate_ui_with_layout(
                                Vec2::new(width, rows as f32 * catalog::ROW_HEIGHT),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.;
                                    for row in 0..rows {
                                        let index = column * rows + row;
                                        if let Some(track) = picks.get(index)
                                            && self.song_row(
                                                ui,
                                                ui.id().with(("pick", index)),
                                                track,
                                                false,
                                            ) == Some(catalog::SongAction::Play)
                                        {
                                            self.start_queue(tracks.clone(), index);
                                        }
                                    }
                                },
                            );
                        }
                    });
                    ui.add_space(32.);
                    ui.label(RichText::new("Discover more").size(30.).strong());
                    ui.add_space(16.);
                    self.song_shelf(ui, "discover_more", &tracks);
                }
            });
    }

    fn tracks_ui(&mut self, ui: &mut egui::Ui, from_queue: bool) {
        let colors = crate::theme::palette(ui.ctx());
        let count = if from_queue {
            self.queue.len()
        } else {
            self.tracks.len()
        };
        if count == 0 {
            ui.add_space(20.);
            ui.label(
                RichText::new(if from_queue {
                    "Make it a listening session"
                } else {
                    "Nothing here yet"
                })
                .size(22.)
                .strong(),
            );
            ui.add_space(4.);
            ui.label(
                RichText::new(if from_queue {
                    "Your queue is empty. Play a song to get started."
                } else {
                    "No tracks found. Try another search."
                })
                .color(colors.muted),
            );
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{count} songs")).color(colors.muted));
            if ui
                .add(egui::Button::new("▶  Play all").corner_radius(20))
                .clicked()
            {
                if from_queue {
                    self.play_at(0);
                } else {
                    self.start_queue(self.tracks.clone(), 0);
                }
            }
        });
        ui.add_space(16.);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            egui::ScrollArea::vertical()
                .id_salt(if from_queue {
                    "queue_list"
                } else {
                    "track_list"
                })
                .auto_shrink([false, false])
                .show_rows(ui, catalog::ROW_HEIGHT, count, |ui, range| {
                    for index in range {
                        let track = if from_queue {
                            self.queue[index].clone()
                        } else {
                            self.tracks[index].clone()
                        };
                        if let Some(action) = self.song_row(ui, ui.id().with(index), &track, true) {
                            if action == catalog::SongAction::Play {
                                if from_queue {
                                    self.play_at(index);
                                } else {
                                    self.start_queue(self.tracks.clone(), index);
                                }
                            } else {
                                self.song_action(&track, action);
                            }
                        }
                    }
                });
        });
    }

    fn library_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        if !self.connected {
            ui.label(
                RichText::new("Connect your account to see playlists and liked songs.")
                    .color(colors.muted),
            );
            if ui
                .add_enabled(!self.connecting, egui::Button::new("Continue with Google"))
                .clicked()
            {
                self.connecting = true;
                self.backend.send(Request::Connect {
                    force_login: self.player.authenticated() != Some(true),
                });
            }
            return;
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::Button::new("Playlists")
                    .selected(true)
                    .corner_radius(8),
            );
            if ui
                .add(egui::Button::new("Liked songs").corner_radius(8))
                .clicked()
            {
                self.playlist("LM".into(), "Liked songs".into());
            }
        });
        ui.add_space(24.);
        if let Some(message) = &self.library_error {
            ui.label(RichText::new(message).color(colors.muted));
            if ui
                .add_enabled(!self.library_loading, egui::Button::new("Retry library"))
                .clicked()
            {
                self.request_library();
            }
            ui.add_space(12.);
        }
        if self.playlists.is_empty() {
            if self.library_loading {
                catalog::skeleton_shelf(ui);
            } else if self.library_loaded {
                ui.label(RichText::new("No playlists in your library yet.").color(colors.muted));
            }
            return;
        }
        let playlists = self.playlists.clone();
        let columns = catalog::columns(ui.available_width(), catalog::CARD_ART, catalog::CARD_GAP);
        egui::ScrollArea::vertical()
            .id_salt("library_grid")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(catalog::CARD_GAP);
                for row in playlists.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for playlist in row {
                            if self
                                .media_card(
                                    ui,
                                    ui.id().with(&playlist.id),
                                    &playlist.title,
                                    &playlist.subtitle,
                                    &playlist.cover,
                                    &format!("Open {}", playlist.title),
                                )
                                .clicked()
                            {
                                self.playlist(playlist.id.clone(), playlist.title.clone());
                            }
                        }
                    });
                }
            });
    }
    fn smoke_toggle(&self) {
        if crate::taskbar::smoke_active() {
            if let Some(bar) = &self.taskbar {
                bar.smoke_click(crate::taskbar::Action::Toggle);
            }
        } else {
            self.player.send(PlayerCommand::Toggle);
        }
    }
    fn check_stream(&mut self, ctx: &egui::Context) {
        let Some(mut check) = self.stream_check.take() else {
            return;
        };
        if crate::taskbar::smoke_active() && check.stage == 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            if let Some(bar) = &self.taskbar {
                bar.smoke_show();
            }
        }
        let trace = check.started.elapsed().as_secs() / 5;
        if trace != check.last_trace && check.stage >= 1 {
            check.last_trace = trace;
            println!(
                "Queue check: stage {}, index {:?}, generation {}, time {:.1}/{:.1}, playing {}, ended {}, video {}x{}",
                check.stage,
                self.current,
                self.state.generation,
                self.state.position,
                self.state.duration,
                self.state.playing,
                self.state.ended,
                self.state.video_width,
                self.state.video_height
            );
        }
        if check.started.elapsed().as_secs() > 90
            || (check.stage == 5
                && self.current == Some(0)
                && check
                    .selected
                    .is_some_and(|selected| selected.elapsed().as_secs() > 15))
        {
            eprintln!(
                "Streaming check timed out: stage {}, ready {}, state {}/{}, playing {}, video {}x{}, error {:?}",
                check.stage,
                self.player.is_ready(),
                self.state.position,
                self.state.duration,
                self.state.playing,
                self.state.video_width,
                self.state.video_height,
                self.error
            );
            crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        match check.stage {
            0 if self.player.is_ready()
                && (!crate::taskbar::smoke_active()
                    || self.taskbar.as_ref().is_some_and(|bar| bar.installed())) =>
            {
                if crate::taskbar::smoke_active() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                self.volume = 0.;
                self.player.send(PlayerCommand::Volume(0.));
                self.queue = vec![
                    Track {
                        id: "Fixture0001".into(),
                        title: "Streaming check one".into(),
                        is_video: true,
                        ..Default::default()
                    },
                    Track {
                        id: "Fixture0002".into(),
                        title: "Streaming check two".into(),
                        is_video: true,
                        ..Default::default()
                    },
                ];
                self.queue_original = self.queue.clone();
                self.expanded = true;
                self.video_mode = true;
                self.player.send(PlayerCommand::Video(true));
                check.selected = Some(std::time::Instant::now());
                self.play_at(0);
                check.stage = 1;
                println!(
                    "YouTube engine ready after {:.2}s",
                    check.started.elapsed().as_secs_f64()
                );
            }
            1 if self.state.playing
                && self.state.position > 0.3
                && self.state.video_width > 0
                && (!crate::mixer::smoke_active() || self.player.mixer_verified())
                && !self.state.ad =>
            {
                println!(
                    "First stream started in {:.2}s; video {}x{}",
                    check.selected.expect("selection").elapsed().as_secs_f64(),
                    self.state.video_width,
                    self.state.video_height
                );
                if crate::mixer::smoke_active() {
                    println!(
                        "Windows audio identity verified: RustifiedYoutube, application icon, volume and mute unchanged"
                    );
                    self.player.send(PlayerCommand::Stop);
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.seek_to(30.);
                assert_eq!(self.timeline.position(self.state.position), 30.);
                self.smoke_toggle();
                check.stage = 2;
            }
            2 if !self.state.playing
                && self.state.position >= 29.
                && self.timeline.pending_confirmed() =>
            {
                println!(
                    "Pause and seek verified; reloading the playback document to verify login rebinding"
                );
                if std::env::args().any(|arg| arg == "--smoke-login-rebind") {
                    self.player.begin_login();
                } else {
                    self.player.reconnect();
                }
                check.stage = 7;
            }
            7 if self.player.is_ready()
                && !self.connecting
                && self.player.authenticated() == Some(true)
                && !self.player.is_restoring()
                && !self.state.playing
                && self.state.position >= 29.
                && self.current == Some(0) =>
            {
                println!(
                    "Playback document reauthenticated; paused track, position and mute restored"
                );
                self.smoke_toggle();
                check.stage = 3;
            }
            3 if self.state.playing && self.state.position > 31. && !self.state.ad => {
                if crate::taskbar::smoke_active() && !check.taskbar_checked {
                    if let Some(bar) = &self.taskbar {
                        bar.smoke_click(crate::taskbar::Action::Next);
                    }
                    check.stage = 8;
                } else if self.connected && !check.session_refreshed {
                    self.connecting = true;
                    self.backend.send(Request::Connect {
                        force_login: self.player.authenticated() != Some(true),
                    });
                    check.session_refreshed = true;
                    check.stage = 6;
                } else {
                    self.video_mode = false;
                    self.player.send(PlayerCommand::Video(false));
                    check.stage = 4;
                }
            }
            8 if self.current == Some(1) && self.state.playing && self.state.position > 0.3 => {
                if let Some(bar) = &self.taskbar {
                    bar.smoke_click(crate::taskbar::Action::Previous);
                }
                check.stage = 9;
            }
            9 if self.current == Some(0)
                && self.state.playing
                && self.state.position > 0.3
                && self.state.duration > 31.
                && !self.state.buffering =>
            {
                check.taskbar_checked = true;
                self.seek_to(30.);
                check.stage = 3;
                println!(
                    "Taskbar Previous and Next verified through the native window message handler"
                );
            }
            6 if !self.connecting
                && self.account_status.starts_with("Google account connected") =>
            {
                if self.current != Some(0)
                    || self.state.generation != self.play_request
                    || self.state.position < 31.
                {
                    eprintln!("Session refresh reset playback");
                    crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                println!("Session refreshed without resetting playback");
                check.stage = 3;
            }
            4 if self.state.playing
                && self.state.duration > 1.
                && !self.state.buffering
                && self.state.position > 32.
                && !self.state.ad
                && !self.state.transition =>
            {
                if !self.state.title.to_lowercase().contains("night") {
                    eprintln!("Audio mode selected another track: {}", self.state.title);
                    crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                println!("Audio mode continues playback");
                self.video_mode = true;
                self.player.send(PlayerCommand::Video(true));
                check.selected = Some(std::time::Instant::now());
                self.seek_to(self.state.duration - 1.);
                check.stage = 5;
            }
            5 if (!crate::taskbar::smoke_active()
                || self.taskbar.as_ref().is_some_and(|bar| {
                    bar.synchronized(crate::taskbar::Controls {
                        has_track: true,
                        playing: true,
                        can_next: false,
                    })
                }))
                && self.state.playing
                && self.current == Some(1)
                && self.state.position > 0.3
                && self.state.video_width > 0
                && !self.state.ad =>
            {
                println!(
                    "Next stream started in {:.2}s; video {}x{}",
                    check.selected.expect("selection").elapsed().as_secs_f64(),
                    self.state.video_width,
                    self.state.video_height
                );
                println!(
                    "Authenticated player: {}; advertisements observed: {}",
                    self.player.authenticated() == Some(true),
                    check.saw_ad
                );
                println!(
                    "Streaming, video, audio mode, pause, seek, login rebinding and automatic queue advance checks passed"
                );
                if crate::taskbar::smoke_active() {
                    println!(
                        "Taskbar toolbar: registration, minimized pause/resume, Previous/Next, playing icon and disabled final Next verified"
                    );
                }
                self.player.send(PlayerCommand::Stop);
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            _ => {}
        }
        self.stream_check = Some(check);
    }
    fn check_account(&mut self, ctx: &egui::Context) {
        if crate::account::smoke_active()
            && let Some(bar) = &self.taskbar
        {
            bar.smoke_show();
        }
        let Some(check) = &mut self.account_check else {
            return;
        };
        if check.phase == 2 {
            return;
        }
        if std::env::args().any(|arg| arg == "--smoke-sign-out") {
            if check.phase == 20 && self.player.is_ready() {
                if self.player.authenticated() != Some(true) {
                    eprintln!("Google sign-out check requires an authenticated app profile");
                    crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.connecting = true;
                self.backend.send(Request::BeginGoogleSignOut);
                check.phase = 21;
            }
            if check.phase == 23 {
                let cleared = self.player.authenticated() == Some(false)
                    && !rustified_core::session::native_enabled()
                    && rustified_core::session::load_session()
                        .ok()
                        .flatten()
                        .is_none();
                if cleared {
                    println!(
                        "Google sign-out confirmed; browser is guest and native session is cleared"
                    );
                    check.phase = 2;
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            } else if check.phase == 24 {
                eprintln!("Google sign-out check failed: {}", self.account_status);
                crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if check.started.elapsed() > Duration::from_secs(75) {
                eprintln!("Google sign-out check timed out: {}", self.account_status);
                crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if check.phase == 10 && self.player.is_ready() {
            self.connecting = true;
            self.backend.send(Request::Connect {
                force_login: self.player.authenticated() != Some(true),
            });
            check.started = std::time::Instant::now();
            check.phase = 11;
        }
        if check.phase == 11
            && !self.player.login_open()
            && self.player.authenticated() == Some(true)
            && !self.connecting
            && self.library_loaded
            && !self.history_loading
        {
            println!(
                "Google sign-in returned to the same playback WebView; library and playback authenticated"
            );
            self.quit = true;
            check.phase = 2;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if (check.started.elapsed() > Duration::from_secs(75) && !self.player.login_open())
            || self.account_status.starts_with("Google requires sign-in")
        {
            eprintln!("Account restore check failed: {}", self.account_status);
            crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if check.phase < 10
            && self.connected
            && self.player.authenticated() == Some(true)
            && self.library_loaded
            && !self.history_loading
            && self.account_status.starts_with("Google account connected")
            && !self.connecting
        {
            if check.phase == 0 {
                println!(
                    "Saved account restored; {} playlists and {} recent tracks (names withheld)",
                    self.playlists.len(),
                    self.account_recent.len()
                );
                check.phase = 1;
                self.connecting = true;
                self.backend.send(Request::Connect {
                    force_login: self.player.authenticated() != Some(true),
                });
            } else {
                check.phase = 2;
                println!("Google account refreshed from the app profile without opening login");
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn expanded_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.expanded = false;
            }
            ui.add_space(16.);
            ui.heading("Now playing");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.selectable_label(self.video_mode, "Video").clicked() {
                    self.video_mode = true;
                    self.player.send(PlayerCommand::Video(true));
                }
                if ui.selectable_label(!self.video_mode, "Audio").clicked() {
                    self.video_mode = false;
                    self.player.send(PlayerCommand::Video(false));
                }
            });
        });
        ui.add_space(16.);
        if let Some(message) = self.error.clone() {
            ui.colored_label(colors.accent, message);
            if ui.button("Retry playback").clicked()
                && let Some(index) = self.current
            {
                self.play_at(index);
            }
            ui.add_space(12.);
        }
        let Some(track) = self.now.clone() else {
            ui.label("Play a song to open the player.");
            return;
        };
        let right_width = 300.0_f32.min(ui.available_width() * 0.35);
        let left_width = (ui.available_width() - right_width - 28.0).max(200.0);
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left_width, height),
                Layout::top_down(Align::Center),
                |ui| {
                    let media_height = (left_width * 9.0 / 16.0).min((height - 172.0).max(120.0));
                    if self.video_mode {
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(left_width, media_height),
                            Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 12., Color32::BLACK);
                        self.video_rect = Some(rect);
                    } else {
                        ui.allocate_ui_with_layout(
                            Vec2::new(left_width, media_height),
                            Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                let cover =
                                    self.cover(ui, &track.cover, media_height.min(left_width));
                                self.artwork_volume(ui, cover, true);
                            },
                        );
                    }
                    self.timeline_ui(ui, "expanded_timeline");
                    ui.add_space(12.);
                    ui.add(egui::Label::new(RichText::new(&track.title).size(22.).strong()).wrap());
                    ui.label(RichText::new(&track.artist).size(16.).color(colors.muted));
                    if self.state.buffering {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Starting stream…");
                        });
                    }
                    if self.video_mode && self.state.playing && !self.state.video_available {
                        ui.label(
                            RichText::new("This track has no music video available.")
                                .color(colors.muted),
                        );
                    }
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                Vec2::new(right_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.heading("Up next");
                    let start = self.current.map_or(0, |index| index + 1);
                    let count = self.queue.len().saturating_sub(start);
                    ui.label(
                        RichText::new(format!("{count} tracks in queue"))
                            .small()
                            .color(colors.muted),
                    );
                    ui.add_space(12.);
                    if count == 0 {
                        ui.label(RichText::new("Add songs to keep listening.").color(colors.muted));
                    }
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        egui::ScrollArea::vertical()
                            .id_salt("expanded_queue")
                            .auto_shrink([false, false])
                            .show_rows(ui, catalog::ROW_HEIGHT, count, |ui, range| {
                                for offset in range {
                                    let index = start + offset;
                                    let next = self.queue[index].clone();
                                    if self.song_row(ui, ui.id().with(index), &next, false)
                                        == Some(catalog::SongAction::Play)
                                    {
                                        self.play_at(index);
                                    }
                                }
                            });
                    });
                },
            );
        });
    }
    fn header_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        let search_id = egui::Id::new("global_search");
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
            ui.memory_mut(|memory| memory.request_focus(search_id));
        }
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 60.).clamp(220., 660.);
            let mut submit = false;
            egui::Frame::new()
                .fill(colors.surface)
                .stroke(egui::Stroke::new(1_f32, colors.border))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(width - 24.);
                    ui.horizontal(|ui| {
                        if icons::button(ui, Icon::Search, "Search · Ctrl+K", 28., false).clicked()
                        {
                            submit = true;
                        }
                        let has_query = !self.query.is_empty();
                        let clear_width = if has_query {
                            28. + ui.spacing().item_spacing.x
                        } else {
                            0.
                        };
                        let response = ui.add_sized(
                            [(ui.available_width() - clear_width).max(1.), 28.],
                            egui::TextEdit::singleline(&mut self.query)
                                .id(search_id)
                                .frame(false)
                                .font(egui::TextStyle::Body)
                                .vertical_align(Align::Center)
                                .hint_text("Search songs, albums, artists…"),
                        );
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            submit = true;
                        }
                        if has_query
                            && icons::button(ui, Icon::Close, "Clear search", 28., false).clicked()
                        {
                            self.query.clear();
                            ui.memory_mut(|memory| memory.request_focus(search_id));
                        }
                    });
                });
            if submit && self.ready {
                self.search(self.query.clone());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, Icon::Account, "Account and settings", 36., false)
                    .on_hover_text(&self.account_status)
                    .clicked()
                {
                    self.page = Page::Settings;
                    self.expanded = false;
                }
            });
        });
        if matches!(self.page, Page::Tracks(_)) {
            ui.add_space(18.);
            ui.horizontal(|ui| {
                for (filter, label) in [
                    (SearchFilter::All, "All"),
                    (SearchFilter::Songs, "Songs"),
                    (SearchFilter::Videos, "Videos"),
                ] {
                    if ui
                        .add(
                            egui::Button::new(label)
                                .selected(self.search_filter == filter)
                                .corner_radius(8),
                        )
                        .clicked()
                    {
                        self.search_filter = filter;
                        if !self.query.trim().is_empty() {
                            self.search(self.query.clone());
                        }
                    }
                }
            });
        }
    }
    fn login_ui(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        if !self.player.login_open() {
            return None;
        }
        let available = ctx.content_rect();
        let size = Vec2::new(
            (available.width() - 60.).min(960.),
            (available.height() - 60.).min(740.),
        );
        let mut open = true;
        let mut cancel = false;
        let mut browser_rect = None;
        egui::Window::new("Continue with Google")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_pos(available.center() - size * 0.5)
            .fixed_size(size)
            .show(ctx, |ui| {
                ui.label(
                    "Sign in to the Google account that has your YouTube Premium subscription.",
                );
                ui.add_space(6.);
                if ui.button("Cancel sign-in").clicked() {
                    cancel = true;
                }
                ui.add_space(6.);
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(size.x - 24., (size.y - 110.).max(240.)),
                    Sense::hover(),
                );
                browser_rect = Some(rect);
            });
        if !open || cancel {
            self.player.cancel_login();
            self.connecting = false;
            return None;
        }
        browser_rect
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let colors = crate::theme::palette(ui.ctx());
        ui.add_space(15.);
        ui.heading("Appearance");
        ui.add_space(6.);
        ui.label(
            RichText::new(
                "Choose your theme. Changes apply instantly and are saved automatically.",
            )
            .small()
            .color(colors.muted),
        );
        ui.add_space(12.);
        if let Some(selected) = theme::selector(ui, self.preferences.theme) {
            self.preferences.theme = selected;
            selected.apply(ui.ctx());
        }
        ui.add_space(28.);
        ui.heading("Account");
        ui.add_space(8.);
        ui.label(&self.account_status);
        ui.label(match self.player.authenticated() {
            Some(true) => "Playback account: signed in to YouTube Music",
            Some(false) if self.player.needs_login() => {
                "Playback account: signed out. Reconnect the account with YouTube Premium below."
            }
            Some(false) => "Playback account: guest",
            None => "Playback account: checking…",
        });
        ui.label(
            RichText::new("Disconnect keeps Google signed in; full sign-out removes this app's Google session.")
            .small()
            .color(colors.muted),
        );
        if ui
            .add_enabled(
                !self.connecting,
                egui::Button::new(if self.connecting {
                    "Connecting…"
                } else {
                    if self.player.needs_login() {
                        "Reconnect playback account"
                    } else if self.connected {
                        "Refresh connection"
                    } else {
                        "Continue with Google"
                    }
                }),
            )
            .clicked()
        {
            self.connecting = true;
            self.backend.send(Request::Connect {
                force_login: self.player.needs_login()
                    || (!self.connected && self.player.authenticated() != Some(true)),
            });
        }
        if ui
            .add_enabled(!self.connecting, egui::Button::new("Open Google sign-in"))
            .clicked()
        {
            self.connecting = true;
            self.backend.send(Request::Connect { force_login: true });
        }
        if (self.connected || self.player.needs_login())
            && ui
                .add_enabled(
                    !self.connecting,
                    egui::Button::new("Disconnect native session"),
                )
                .clicked()
        {
            if self.player.login_open() {
                self.player.cancel_login();
            }
            self.backend.send(Request::Disconnect);
            self.connecting = false;
            self.playlists.clear();
        }
        if (self.connected || self.player.authenticated() == Some(true))
            && ui
                .add_enabled(
                    !self.connecting,
                    egui::Button::new("Sign out of Google in this app"),
                )
                .clicked()
        {
            self.connecting = true;
            self.backend.send(Request::BeginGoogleSignOut);
        }
        ui.add_space(28.);
        ui.heading("Playback");
        ui.label("Playback: official YouTube Music streaming engine");
        let mut step = self
            .preferences
            .volume_scroll_step
            .unwrap_or(5)
            .clamp(1, 100);
        ui.horizontal(|ui| {
            ui.label("Volume step per scroll notch");
            if ui
                .add(egui::DragValue::new(&mut step).range(1..=100).suffix(" %"))
                .changed()
            {
                self.preferences.volume_scroll_step = Some(step);
            }
        });
        let mut close_to_tray = self.preferences.close_to_tray.unwrap_or(true);
        if ui
            .checkbox(
                &mut close_to_tray,
                "Keep playing in the tray when I close the window",
            )
            .changed()
        {
            self.preferences.close_to_tray = Some(close_to_tray);
        }
        ui.add_space(28.);
        ui.heading("Discord Rich Presence");
        let mut enabled = self.preferences.discord_rpc.unwrap_or(true);
        if ui
            .checkbox(&mut enabled, "Show what I'm listening to on Discord")
            .changed()
        {
            self.preferences.discord_rpc = Some(enabled);
        }
        ui.label(RichText::new("Shares the song, artist, artwork and progress while playing. Hides on pause or during ads.").small().color(colors.muted));
        ui.label(RichText::new(&self.discord.status).color(if enabled {
            colors.accent
        } else {
            colors.muted
        }));
        ui.collapsing("Application settings", |ui| {
            let mut id = self
                .preferences
                .discord_application_id
                .clone()
                .unwrap_or_else(|| crate::discord::APPLICATION_ID.into());
            ui.horizontal(|ui| {
                ui.label("Discord Application ID");
                if ui
                    .add(egui::TextEdit::singleline(&mut id).desired_width(220.))
                    .changed()
                {
                    self.preferences.discord_application_id = Some(id.trim().into());
                }
            });
            if !crate::discord::valid_application_id(&id) {
                ui.colored_label(colors.accent, "Use the public numeric Application ID.");
            }
            ui.hyperlink_to(
                "Discord Developer Portal",
                "https://discord.com/developers/applications",
            );
        });
        ui.add_space(28.);
        ui.heading("RustifiedYoutube");
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        if ui.button("About RustifiedYoutube").clicked() {
            self.about = true;
        }
        if ui.button("Quit application").clicked() {
            self.quit = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for MusicApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let colors = crate::theme::palette(ctx);
        self.window_guard.check_startup(ctx);
        self.receive(ctx);
        self.retry_account_data(ctx);
        self.check_stream(ctx);
        self.check_account(ctx);
        self.discord.update(
            self.preferences.discord_rpc.unwrap_or(true)
                && !std::env::args().any(|arg| arg == "--smoke-stream")
                && !crate::account::smoke_active()
                && !crate::preview::active(),
            self.preferences
                .discord_application_id
                .as_deref()
                .unwrap_or(crate::discord::APPLICATION_ID),
            self.now.as_ref(),
            &self.state,
        );
        if let Some(taskbar) = &mut self.taskbar {
            let controls = crate::taskbar::Controls {
                has_track: self.now.is_some()
                    && !self.player.login_open()
                    && !self.player.needs_login(),
                playing: self.state.playing,
                can_next: self.current.is_some_and(|index| {
                    index + 1 < self.queue.len() || (self.repeat && !self.queue.is_empty())
                }),
            };
            let tooltip = self
                .now
                .as_ref()
                .map(|track| format!("{} — {}", track.title, track.artist))
                .unwrap_or_else(|| "RustifiedYoutube".into());
            taskbar.update(controls, &tooltip);
        }
        self.video_rect = None;
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit
            && self.preferences.close_to_tray.unwrap_or(true)
            && self.tray.is_some()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Q)) {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::CTRL | egui::Modifiers::ALT,
                egui::Key::Space,
            )
        }) {
            self.player.send(PlayerCommand::Toggle);
        }
        self.player_bar(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(30, 24)),
            )
            .show(ctx, |ui| {
                let backdrop = ui.max_rect().expand2(Vec2::new(30., 24.));
                let fade =
                    egui::Rect::from_min_size(backdrop.min, Vec2::new(backdrop.width(), 220.));
                let mut ambient = egui::Mesh::default();
                let top = colors.panel.gamma_multiply(1.8);
                ambient.colored_vertex(fade.left_top(), top);
                ambient.colored_vertex(fade.right_top(), top);
                ambient.colored_vertex(fade.right_bottom(), colors.background);
                ambient.colored_vertex(fade.left_bottom(), colors.background);
                ambient.add_triangle(0, 1, 2);
                ambient.add_triangle(0, 2, 3);
                ui.painter().add(egui::Shape::mesh(ambient));
                if self.expanded {
                    self.expanded_ui(ui);
                    return;
                }
                self.header_ui(ui);
                ui.add_space(28.);
                if self.page != Page::Discover {
                    let title = match &self.page {
                        Page::Library => "Your library",
                        Page::Queue => "Up next",
                        Page::Settings => "Settings",
                        Page::Tracks(title) => title.as_str(),
                        Page::Discover => "",
                    };
                    ui.label(RichText::new(title).size(30.).strong());
                    ui.add_space(20.);
                }
                if let Some(message) = self.error.clone() {
                    egui::Frame::new()
                        .fill(colors.error)
                        .corner_radius(8)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(message);
                                if self.now.is_some() && ui.small_button("Retry playback").clicked()
                                {
                                    self.error = None;
                                    self.player.reconnect();
                                }
                                if ui.small_button("Google sign-in").clicked() {
                                    self.connecting = true;
                                    self.backend.send(Request::Connect { force_login: true });
                                }
                                if ui.small_button("Dismiss").clicked() {
                                    self.error = None;
                                }
                            });
                        });
                    ui.add_space(8.);
                }
                match self.page.clone() {
                    Page::Discover => self.discover_ui(ui),
                    Page::Library => self.library_ui(ui),
                    Page::Queue => self.tracks_ui(ui, true),
                    Page::Settings => {
                        egui::ScrollArea::vertical().show(ui, |ui| self.settings_ui(ui));
                    }
                    Page::Tracks(_) => {
                        if self.loading {
                            ui.label(RichText::new("Loading songs…").color(colors.muted));
                            ui.add_space(12.);
                            let rows =
                                (ui.available_height() / catalog::ROW_HEIGHT).floor() as usize;
                            catalog::skeleton_rows(ui, rows.clamp(1, 8));
                        } else {
                            self.tracks_ui(ui, false);
                        }
                    }
                }
            });
        egui::Window::new("About RustifiedYoutube")
            .open(&mut self.about)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("RustifiedYoutube");
                ui.label(
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .color(colors.accent),
                );
                ui.label("Your music. Your desktop.");
                ui.add_space(12.);
                ui.label("Rust interface · YouTube Music streaming · WebView2 media engine");
                ui.hyperlink_to(
                    "GitHub project",
                    "https://github.com/zygerr/RustifiedYoutube",
                );
            });
        if self.connecting {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        let login_rect = self.login_ui(ctx);
        self.player.place_video(
            if self.player.login_open() {
                login_rect
            } else if self.about {
                None
            } else {
                self.video_rect
            },
            ctx.pixels_per_point(),
        );
        if self.stream_check.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.now.is_some() && !self.state.playing {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if let Some(snapshot) = &mut self.snapshot
            && snapshot.finish(ctx)
        {
            self.quit = true;
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if std::env::args().any(|arg| arg == "--smoke-stream")
            || crate::preview::active()
            || crate::account::smoke_active()
        {
            return;
        }
        self.window_guard.repair(storage);
        self.preferences.volume = Some(self.volume);
        eframe::set_value(storage, "preferences", &self.preferences);
    }
}
