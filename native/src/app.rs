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

const ACCENT: Color32 = Color32::from_rgb(244, 148, 114);
const BG: Color32 = Color32::from_rgb(17, 19, 24);
const PANEL: Color32 = Color32::from_rgb(23, 26, 33);
const MUTED: Color32 = Color32::from_rgb(144, 151, 166);
const TEXT: Color32 = Color32::from_rgb(239, 241, 245);

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
    volume: Option<f32>,
    recent: Vec<Track>,
    close_to_tray: Option<bool>,
    volume_scroll_step: Option<u8>,
}

struct StreamCheck {
    started: std::time::Instant,
    selected: Option<std::time::Instant>,
    stage: u8,
    last_trace: u64,
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
    state: PlayerState,
    page: Page,
    query: String,
    search_filter: SearchFilter,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    queue: Vec<Track>,
    current: Option<usize>,
    now: Option<Track>,
    loading: bool,
    resolving: bool,
    preparation: String,
    connecting: bool,
    connected: bool,
    ready: bool,
    error: Option<String>,
    list_request: u64,
    play_request: u64,
    volume: f32,
    volume_wheel: crate::volume::Wheel,
    repeat: bool,
    preferences: Preferences,
    textures: HashMap<String, egui::TextureHandle>,
    texture_order: VecDeque<String>,
    requested: HashSet<String>,
    tray: Option<tray_icon::TrayIcon>,
    tray_rx: mpsc::Receiver<TrayAction>,
    quit: bool,
    about: bool,
    seeking: bool,
    seek_position: f64,
    expanded: bool,
    video_mode: bool,
    video_rect: Option<egui::Rect>,
    stream_check: Option<StreamCheck>,
}

impl MusicApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        crate::fonts::install(ctx);
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.25);
        style.visuals.selection.stroke.color = ACCENT;
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 38, 48);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 51, 63);
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(14., 9.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.));
        ctx.set_style(style);
        let preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        let volume = preferences.volume.unwrap_or(0.5);
        let (tray_tx, tray_rx) = mpsc::channel();
        let tray = Self::create_tray(ctx.clone(), tray_tx).ok();
        let player = Player::start(cc);
        player.send(PlayerCommand::Volume(volume as f64));
        Self {
            backend: Backend::start(ctx.clone()),
            player,
            state: PlayerState::default(),
            page: Page::Discover,
            query: String::new(),
            search_filter: SearchFilter::All,
            tracks: Vec::new(),
            playlists: Vec::new(),
            queue: Vec::new(),
            current: None,
            now: None,
            loading: true,
            resolving: false,
            preparation: String::new(),
            connecting: false,
            connected: false,
            ready: false,
            error: None,
            list_request: 0,
            play_request: 0,
            volume,
            volume_wheel: crate::volume::Wheel::default(),
            repeat: false,
            preferences,
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            requested: HashSet::new(),
            tray,
            tray_rx,
            quit: false,
            about: false,
            seeking: false,
            seek_position: 0.,
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
                }),
        }
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
        self.seeking = false;
        self.seek_position = 0.;
        self.state = PlayerState::default();
        self.error = None;
        self.player.send(PlayerCommand::Stop);
        self.state.buffering = true;
        self.player.send(PlayerCommand::Play {
            generation: self.play_request,
            video_id: track.id.clone(),
        });
        self.preferences.recent.retain(|item| item.id != track.id);
        self.preferences.recent.insert(0, track);
        self.preferences.recent.truncate(30);
    }

    fn next(&mut self) {
        if let Some(index) = self.current {
            if index + 1 < self.queue.len() {
                self.play_at(index + 1);
            } else if self.repeat && !self.queue.is_empty() {
                self.play_at(0);
            }
        }
    }

    fn previous(&mut self) {
        if self.state.position > 3. {
            self.player.send(PlayerCommand::Seek(0.));
        } else if let Some(index) = self.current {
            self.play_at(index.saturating_sub(1));
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.player.pump();
        if self.player.take_quit() {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        while let Ok(response) = self.backend.rx.try_recv() {
            match response {
                Response::Ready(connected) => {
                    if self.connecting {
                        self.player.send(PlayerCommand::Reload);
                    }
                    self.connected = connected;
                    self.connecting = false;
                    self.ready = true;
                    if connected {
                        self.backend.send(Request::Library);
                    }
                    if self.tracks.is_empty() {
                        self.list_request += 1;
                        self.backend.send(Request::Search(
                            self.list_request,
                            "popular music".into(),
                            SearchFilter::All,
                        ));
                    }
                }
                Response::Tracks(id, result) if id == self.list_request => {
                    self.loading = false;
                    match result {
                        Ok(tracks) => self.tracks = tracks,
                        Err(message) => self.error = Some(message),
                    }
                }
                Response::Library(result) => match result {
                    Ok(playlists) => self.playlists = playlists,
                    Err(message) => self.error = Some(message),
                },
                Response::Liked(id, liked, success) => {
                    if success {
                        for track in self
                            .tracks
                            .iter_mut()
                            .chain(self.queue.iter_mut())
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
                            "Couldn't update liked songs. Try reconnecting your account.".into(),
                        );
                    }
                }
                Response::Cover(url, image) => {
                    if self.textures.len() >= 128
                        && let Some(old) = self.texture_order.pop_front()
                    {
                        self.textures.remove(&old);
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
                    self.error = Some(message);
                }
                _ => {}
            }
        }
        while let Ok(state) = self.player.rx.try_recv() {
            if self.resolving || (state.generation != self.play_request && state.generation != 0) {
                continue;
            }
            let ended = state.ended;
            if let Some(error) = &state.error {
                self.error = Some(error.clone());
            }
            self.state = state;
            if ended {
                self.next();
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

    fn cover(&mut self, ui: &mut egui::Ui, url: &str, size: f32) {
        if let Some(texture) = self.textures.get(url) {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(Vec2::splat(size))
                    .corner_radius(6),
            );
        } else {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            ui.painter()
                .rect_filled(rect, 6., Color32::from_rgb(41, 45, 56));
            icons::draw(
                ui.painter(),
                rect.shrink(size * 0.25),
                Icon::Play,
                MUTED.gamma_multiply(0.5),
            );
            if !url.is_empty() && self.requested.insert(url.into()) {
                self.backend.send(Request::Cover(url.into()));
            }
        }
    }

    fn nav(ui: &mut egui::Ui, text: &str, icon: Icon, selected: bool) -> bool {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                8.,
                if selected {
                    Color32::from_rgb(53, 40, 36)
                } else {
                    Color32::from_rgb(32, 36, 44)
                },
            );
        }
        let color = if selected { ACCENT } else { MUTED };
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
            FontId::proportional(14.),
            color,
        );
        response.clicked()
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .exact_width(224.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(14, 16, 21))
                    .inner_margin(20),
            )
            .show(ctx, |ui| {
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                    ui.painter().circle_stroke(
                        rect.center(),
                        12.,
                        egui::Stroke::new(2_f32, ACCENT),
                    );
                    icons::draw(ui.painter(), rect.shrink(6.), Icon::Play, ACCENT);
                    ui.label(RichText::new("RUSTIFIED").size(18.).strong());
                });
                ui.add_space(3.);
                ui.label(RichText::new("Your music, closer.").size(11.).color(MUTED));
                ui.add_space(32.);
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
                    if self.connected {
                        self.backend.send(Request::Library);
                    }
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
                ui.add_space(24.);
                ui.label(
                    RichText::new("YOUR PLAYLISTS")
                        .size(10.)
                        .color(MUTED)
                        .strong(),
                );
                ui.add_space(8.);
                let playlists = self.playlists.clone();
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_playlists")
                    .max_height((ui.available_height() - 190.).max(80.))
                    .show(ui, |ui| {
                        for playlist in playlists {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(&playlist.title).color(MUTED))
                                        .frame(false),
                                )
                                .clicked()
                            {
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
                                .color(MUTED),
                            );
                        }
                    });
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    if ui
                        .button(
                            RichText::new(format!("v{}  ·  About", env!("CARGO_PKG_VERSION")))
                                .small()
                                .color(MUTED),
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
                    if self.connected {
                        ui.label(
                            RichText::new("●  Google account connected")
                                .small()
                                .color(ACCENT),
                        );
                    } else if ui
                        .add_enabled(
                            !self.connecting,
                            egui::Button::new(if self.connecting {
                                "Connecting…"
                            } else {
                                "Connect Google"
                            }),
                        )
                        .clicked()
                    {
                        self.connecting = true;
                        self.backend.send(Request::Connect);
                    }
                });
            });
    }

    fn player_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("player")
            .exact_height(110.)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(24, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let now_card = ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width() * 0.31).max(230.), 72.),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            if let Some(track) = self.now.clone() {
                                self.cover(ui, &track.cover, 58.);
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&track.title).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(&track.title);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&track.artist).small().color(MUTED),
                                        )
                                        .truncate(),
                                    );
                                });
                            } else {
                                ui.label(RichText::new("Find your next favorite.").color(MUTED));
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
                        Vec2::new((ui.available_width() - 240.).max(260.), 78.),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(((ui.available_width() - 170.) / 2.).max(0.));
                                if icons::button(ui, Icon::Previous, "Previous track", 36., false)
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
                                    } else if !self.tracks.is_empty() {
                                        self.queue = self.tracks.clone();
                                        self.play_at(0);
                                    }
                                }
                                if icons::button(ui, Icon::Next, "Next track", 36., false).clicked()
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
                                        ACCENT,
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                if self.resolving {
                                    ui.label(RichText::new(&self.preparation).small().color(MUTED));
                                    return;
                                }
                                ui.label(
                                    RichText::new(format_time(self.state.position))
                                        .size(10.)
                                        .color(MUTED),
                                );
                                if !self.seeking {
                                    self.seek_position = self.state.position;
                                }
                                let response = ui.add_sized(
                                    [(ui.available_width() - 48.).max(80.), 18.],
                                    egui::Slider::new(
                                        &mut self.seek_position,
                                        0.0..=self.state.duration.max(1.),
                                    )
                                    .clamping(egui::SliderClamping::Never)
                                    .show_value(false),
                                );
                                if response.drag_started() {
                                    self.seeking = true;
                                }
                                if response.drag_stopped()
                                    || (response.changed() && !response.dragged())
                                {
                                    self.seeking = false;
                                    self.player.send(PlayerCommand::Seek(self.seek_position));
                                }
                                ui.label(
                                    RichText::new(format_time(self.state.duration))
                                        .size(10.)
                                        .color(MUTED),
                                );
                            });
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icons::button(ui, Icon::Queue, "Open play queue", 32., false).clicked() {
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
                        icons::draw(
                            ui.painter(),
                            egui::Rect::from_center_size(
                                ui.cursor().left_center(),
                                Vec2::splat(20.),
                            ),
                            Icon::Volume,
                            MUTED,
                        );
                    });
                });
            });
    }

    fn tracks_ui(&mut self, ui: &mut egui::Ui, from_queue: bool) {
        let count = if from_queue {
            self.queue.len()
        } else {
            self.tracks.len()
        };
        if count == 0 {
            ui.add_space(38.);
            ui.label(
                RichText::new(if from_queue {
                    "Your queue is empty. Play a song to get started."
                } else {
                    "No tracks found. Try another search."
                })
                .color(MUTED),
            );
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{count} songs")).small().color(MUTED));
            ui.add_space(10.);
            if ui.button("Play all").clicked() {
                if !from_queue {
                    self.queue = self.tracks.clone();
                }
                self.play_at(0);
            }
        });
        ui.add_space(12.);
        egui::ScrollArea::vertical()
            .id_salt("track_list")
            .auto_shrink([false, false])
            .show_rows(ui, 66., count, |ui, range| {
                for index in range {
                    let track = if from_queue {
                        self.queue[index].clone()
                    } else {
                        self.tracks[index].clone()
                    };
                    let active = self.now.as_ref().is_some_and(|t| t.id == track.id);
                    egui::Frame::new()
                        .fill(if active {
                            Color32::from_rgb(43, 35, 33)
                        } else {
                            BG
                        })
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_height(50.);
                            ui.horizontal(|ui| {
                                if icons::button(
                                    ui,
                                    Icon::Play,
                                    &format!("Play {}", track.title),
                                    28.,
                                    false,
                                )
                                .clicked()
                                {
                                    if !from_queue {
                                        self.queue = self.tracks.clone();
                                    }
                                    self.play_at(index);
                                }
                                self.cover(ui, &track.cover, 46.);
                                let title_width = (ui.available_width() - 220.).max(150.);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(title_width, 46.),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&track.title)
                                                    .color(if active { ACCENT } else { TEXT })
                                                    .strong(),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&track.title);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{} · {}",
                                                    if track.is_video { "Video" } else { "Song" },
                                                    track.artist
                                                ))
                                                .color(MUTED)
                                                .size(12.),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if icons::button(ui, Icon::Plus, "Add to queue", 28., false)
                                        .clicked()
                                    {
                                        self.queue.push(track.clone());
                                    }
                                    let heart = icons::button(
                                        ui,
                                        Icon::Heart,
                                        if track.liked {
                                            "Unlike song"
                                        } else {
                                            "Like song"
                                        },
                                        28.,
                                        false,
                                    );
                                    if track.liked {
                                        ui.painter().circle_filled(
                                            heart.rect.right_top() - Vec2::new(3., -3.),
                                            2.,
                                            ACCENT,
                                        );
                                    }
                                    if heart.clicked() {
                                        if self.connected {
                                            self.backend.send(Request::Like(
                                                track.id.clone(),
                                                !track.liked,
                                            ));
                                        } else {
                                            self.error = Some(
                                                "Connect your Google account to like songs.".into(),
                                            );
                                        }
                                    }
                                    ui.label(RichText::new(&track.duration).color(MUTED).size(12.));
                                });
                            });
                        });
                }
            });
    }

    fn library_ui(&mut self, ui: &mut egui::Ui) {
        if !self.connected {
            ui.add_space(24.);
            ui.label("Bring your playlists and liked songs with you.");
            if ui
                .add_enabled(!self.connecting, egui::Button::new("Connect Google"))
                .clicked()
            {
                self.connecting = true;
                self.backend.send(Request::Connect);
            }
            return;
        }
        if self.playlists.is_empty() {
            ui.label("No playlists to show. If your library couldn't load, reconnect your account in Settings.");
            return;
        }
        let playlists = self.playlists.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for playlist in playlists {
                    let card = egui::Frame::new()
                        .fill(PANEL)
                        .corner_radius(10)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_width(164.);
                            self.cover(ui, &playlist.cover, 164.);
                            ui.add_space(6.);
                            ui.add(
                                egui::Label::new(RichText::new(&playlist.title).strong())
                                    .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&playlist.subtitle).size(11.).color(MUTED),
                                )
                                .truncate(),
                            );
                        });
                    if card
                        .response
                        .interact(Sense::click())
                        .on_hover_text("Open playlist")
                        .clicked()
                    {
                        self.playlist(playlist.id, playlist.title);
                    }
                }
            });
        });
    }

    fn check_stream(&mut self, ctx: &egui::Context) {
        let Some(mut check) = self.stream_check.take() else {
            return;
        };
        let trace = check.started.elapsed().as_secs() / 5;
        if trace != check.last_trace && check.stage >= 5 {
            check.last_trace = trace;
            println!(
                "Queue check: index {:?}, generation {}, time {:.1}/{:.1}, playing {}, ended {}, video {}x{}",
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
            0 if self.player.is_ready() => {
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
                && !self.state.ad =>
            {
                println!(
                    "First stream started in {:.2}s; video {}x{}",
                    check.selected.expect("selection").elapsed().as_secs_f64(),
                    self.state.video_width,
                    self.state.video_height
                );
                self.player.send(PlayerCommand::Seek(30.));
                self.player.send(PlayerCommand::Toggle);
                check.stage = 2;
            }
            2 if !self.state.playing && self.state.position >= 29. => {
                println!("Pause and seek verified");
                self.player.send(PlayerCommand::Toggle);
                check.stage = 3;
            }
            3 if self.state.playing && self.state.position > 31. && !self.state.ad => {
                self.video_mode = false;
                self.player.send(PlayerCommand::Video(false));
                check.stage = 4;
            }
            4 if self.state.playing
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
                self.player
                    .send(PlayerCommand::Seek(self.state.duration - 1.));
                check.stage = 5;
            }
            5 if self.state.playing
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
                    "Streaming, video, audio mode, pause, seek and automatic queue advance checks passed"
                );
                self.player.send(PlayerCommand::Stop);
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            _ => {}
        }
        self.stream_check = Some(check);
    }
    fn expanded_ui(&mut self, ui: &mut egui::Ui) {
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
            ui.colored_label(ACCENT, message);
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
                    let media_height = (left_width * 9.0 / 16.0).min((height - 130.0).max(160.0));
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
                                self.cover(ui, &track.cover, media_height.min(left_width));
                            },
                        );
                    }
                    ui.add_space(20.);
                    ui.add(egui::Label::new(RichText::new(&track.title).size(22.).strong()).wrap());
                    ui.label(RichText::new(&track.artist).size(16.).color(MUTED));
                    if self.state.buffering {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Starting stream…");
                        });
                    }
                    if self.video_mode && self.state.playing && !self.state.video_available {
                        ui.label(
                            RichText::new("This track has no music video available.").color(MUTED),
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
                            .color(MUTED),
                    );
                    ui.add_space(12.);
                    if count == 0 {
                        ui.label(RichText::new("Add songs to keep listening.").color(MUTED));
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("expanded_queue")
                        .auto_shrink([false, false])
                        .show_rows(ui, 76., count, |ui, range| {
                            for offset in range {
                                let index = start + offset;
                                let next = self.queue[index].clone();
                                let row = egui::Frame::new()
                                    .fill(PANEL)
                                    .corner_radius(8)
                                    .inner_margin(8)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            self.cover(ui, &next.cover, 46.);
                                            ui.allocate_ui_with_layout(
                                                Vec2::new((right_width - 88.).max(90.), 48.),
                                                Layout::top_down(Align::Min),
                                                |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&next.title).strong(),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(&next.title);
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&next.artist)
                                                                .small()
                                                                .color(MUTED),
                                                        )
                                                        .truncate(),
                                                    );
                                                },
                                            );
                                        });
                                    });
                                let response = row
                                    .response
                                    .interact(Sense::click())
                                    .on_hover_text("Play this track");
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        format!("Play {}", next.title),
                                    )
                                });
                                if response.clicked() {
                                    self.play_at(index);
                                }
                            }
                        });
                },
            );
        });
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(15.);
        ui.heading("Account");
        ui.add_space(8.);
        ui.label(if self.connected {
            "Your Google account is connected."
        } else {
            "Search and listen without an account, or connect your library."
        });
        if ui
            .add_enabled(
                !self.connecting,
                egui::Button::new(if self.connecting {
                    "Connecting…"
                } else {
                    "Connect / reconnect Google"
                }),
            )
            .clicked()
        {
            self.connecting = true;
            self.backend.send(Request::Connect);
        }
        if self.connected && ui.button("Disconnect native session").clicked() {
            self.backend.send(Request::Disconnect);
            self.playlists.clear();
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
        self.receive(ctx);
        self.check_stream(ctx);
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
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(30, 24)),
            )
            .show(ctx, |ui| {
                if self.expanded {
                    self.expanded_ui(ui);
                    return;
                }
                ui.horizontal(|ui| {
                    let search = ui.add_sized(
                        [(ui.available_width() - 110.).max(120.), 36.],
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search songs, artists, albums…")
                            .margin(Vec2::new(12., 10.)),
                    );
                    if (search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui
                            .add_enabled(
                                self.ready,
                                egui::Button::new("Search").min_size(Vec2::new(90., 36.)),
                            )
                            .clicked()
                    {
                        self.search(self.query.clone());
                    }
                });
                ui.horizontal(|ui| {
                    for (filter, label) in [
                        (SearchFilter::All, "All"),
                        (SearchFilter::Songs, "Songs"),
                        (SearchFilter::Videos, "Videos"),
                    ] {
                        if ui
                            .selectable_label(self.search_filter == filter, label)
                            .clicked()
                        {
                            self.search_filter = filter;
                            if !self.query.trim().is_empty() {
                                self.search(self.query.clone());
                            }
                        }
                    }
                });
                ui.add_space(24.);
                let title = match &self.page {
                    Page::Discover => "Make room for your music.",
                    Page::Library => "Your library",
                    Page::Queue => "Up next",
                    Page::Settings => "Settings",
                    Page::Tracks(title) => title.as_str(),
                };
                ui.label(RichText::new(title).size(28.).strong());
                ui.add_space(12.);
                if let Some(message) = self.error.clone() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(64, 37, 34))
                        .corner_radius(8)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&message);
                                if ui.small_button("Dismiss").clicked() {
                                    self.error = None;
                                }
                            });
                        });
                    ui.add_space(8.);
                }
                match self.page.clone() {
                    Page::Discover => {
                        ui.label(
                            RichText::new(
                                "A little focus, a little energy, or something unexpected.",
                            )
                            .color(MUTED),
                        );
                        ui.add_space(12.);
                        ui.horizontal_wrapped(|ui| {
                            for (label, query) in [
                                ("Focus", "lofi focus"),
                                ("Energy", "workout music"),
                                ("Unwind", "chill acoustic"),
                                ("Romanian", "Romanian music"),
                                ("Rock", "rock classics"),
                            ] {
                                if ui.button(label).clicked() {
                                    self.query = query.into();
                                    self.search(query.into());
                                }
                            }
                        });
                        ui.add_space(20.);
                        if self.loading {
                            ui.spinner();
                            ui.label("Finding your music…");
                        } else {
                            self.tracks_ui(ui, false);
                        }
                    }
                    Page::Library => self.library_ui(ui),
                    Page::Queue => self.tracks_ui(ui, true),
                    Page::Settings => self.settings_ui(ui),
                    Page::Tracks(_) => {
                        if self.loading {
                            ui.spinner();
                            ui.label("Loading tracks…");
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
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(ACCENT),
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
        self.player.place_video(
            if self.about { None } else { self.video_rect },
            ctx.pixels_per_point(),
        );
        if self.stream_check.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.now.is_some() && !self.state.playing {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if std::env::args().any(|arg| arg == "--smoke-stream") {
            return;
        }
        self.preferences.volume = Some(self.volume);
        eframe::set_value(storage, "preferences", &self.preferences);
    }
}
