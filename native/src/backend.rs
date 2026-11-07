use eframe::egui;
use rustified_core::{
    api::Catalog,
    model::{Playlist, Track},
    resolver, session,
};
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    time::Duration,
};
use tokio::sync::{RwLock, Semaphore};

pub enum Request {
    Search(u64, String),
    Playlist(u64, String),
    Library,
    Resolve(u64, String),
    Connect,
    Disconnect,
    Like(String, bool),
    Cover(String),
}

pub enum Response {
    Ready(bool),
    Tracks(u64, Result<Vec<Track>, String>),
    Library(Result<Vec<Playlist>, String>),
    Stream(u64, Result<resolver::PreparedAudio, String>),
    Liked(String, bool, bool),
    Cover(String, egui::ColorImage),
    Notice(String),
}

pub struct Backend {
    pub tx: tokio::sync::mpsc::UnboundedSender<Request>,
    pub rx: mpsc::Receiver<Response>,
}

fn send(tx: &mpsc::Sender<Response>, ctx: &egui::Context, response: Response) {
    if tx.send(response).is_ok() {
        ctx.request_repaint();
    }
}

impl Backend {
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, mut requests) = tokio::sync::mpsc::unbounded_channel();
        let (updates, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("background runtime");
            runtime.block_on(async move {
                let mut cookie = session::load_cookie().ok().flatten();
                let client = match Catalog::new(cookie.as_deref()).await {
                    Ok(api) => api,
                    Err(_) => {
                        cookie = None;
                        send(&updates, &ctx, Response::Notice("Couldn't reconnect. You can still search, or connect your Google account again.".into()));
                        match Catalog::new(None).await {
                            Ok(api) => api,
                            Err(_) => { send(&updates, &ctx, Response::Notice("Couldn't reach YouTube Music. Check your internet connection and restart the app.".into())); return; }
                        }
                    }
                };
                let signed_in = client.signed_in();
                let api = Arc::new(RwLock::new(Arc::new(client)));
                let cookie = Arc::new(RwLock::new(cookie));
                send(&updates, &ctx, Response::Ready(signed_in));
                let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("image client");
                let images = Arc::new(Semaphore::new(4));
                let mut resolver_job: Option<tokio::task::JoinHandle<()>> = None;
                let mut search_jobs: HashMap<u64, tokio::task::JoinHandle<()>> = HashMap::new();
                while let Some(request) = requests.recv().await {
                    let api = api.clone();
                    let cookie = cookie.clone();
                    let updates = updates.clone();
                    let ctx = ctx.clone();
                    let http = http.clone();
                    let images = images.clone();
                    if matches!(request, Request::Resolve(..))
                        && let Some(job) = resolver_job.take() { job.abort(); }
                    let is_resolve = matches!(request, Request::Resolve(..));
                    let search_id = match &request { Request::Search(id, _) | Request::Playlist(id, _) => Some(*id), _ => None };
                    if search_id.is_some() {
                        for (_, job) in search_jobs.drain() { job.abort(); }
                    }
                    let job = tokio::spawn(async move {
                        match request {
                            Request::Search(id, query) => {
                                let client = api.read().await.clone();
                                let result = client.search(&query).await.map_err(|_| "Search couldn't load. Check your connection and try again.".into());
                                send(&updates, &ctx, Response::Tracks(id, result));
                            }
                            Request::Playlist(id, playlist) => {
                                let client = api.read().await.clone();
                                let result = client.playlist(&playlist).await.map_err(|_| "Couldn't load this playlist. Reconnect your Google account and try again.".into());
                                send(&updates, &ctx, Response::Tracks(id, result));
                            }
                            Request::Library => {
                                let client = api.read().await.clone();
                                let result = client.library().await.map_err(|_| "Your library couldn't load. Connect or reconnect your Google account.".into());
                                send(&updates, &ctx, Response::Library(result));
                            }
                            Request::Resolve(id, video) => {
                                let cookie = cookie.read().await.clone();
                                let result = resolver::prepare(&video, cookie.as_deref()).await.map_err(|e| e.to_string());
                                send(&updates, &ctx, Response::Stream(id, result));
                            }
                            Request::Like(id, liked) => {
                                let client = api.read().await.clone();
                                let success = client.like(&id, liked).await.is_ok();
                                send(&updates, &ctx, Response::Liked(id, liked, success));
                            }
                            Request::Connect => {
                                let result = tokio::process::Command::new(session::adjacent_tool("rustified-auth.exe")).kill_on_drop(true).status().await;
                                if result.is_ok_and(|s| s.success())
                                    && let Ok(Some(new_cookie)) = session::load_cookie()
                                        && let Ok(client) = Catalog::new(Some(&new_cookie)).await {
                                            *api.write().await = Arc::new(client);
                                            *cookie.write().await = Some(new_cookie);
                                            send(&updates, &ctx, Response::Ready(true));
                                            return;
                                        }
                                send(&updates, &ctx, Response::Notice("Connection wasn't completed. You can try again whenever you're ready.".into()));
                            }
                            Request::Disconnect => {
                                if session::disconnect().is_ok()
                                    && let Ok(client) = Catalog::new(None).await {
                                        *api.write().await = Arc::new(client);
                                        *cookie.write().await = None;
                                        send(&updates, &ctx, Response::Ready(false));
                                    }
                            }
                            Request::Cover(url) => {
                                let Ok(parsed) = reqwest::Url::parse(&url) else { return; };
                                if parsed.scheme() != "https" || !parsed.host_str().is_some_and(|h| h.ends_with(".ytimg.com") || h.ends_with(".ggpht.com") || h.ends_with(".googleusercontent.com")) { return; }
                                let Ok(_permit) = images.acquire().await else { return; };
                                let Ok(response) = http.get(&url).send().await else { return; };
                                if !response.status().is_success() || response.content_length().is_some_and(|s| s > 2_000_000) { return; }
                                let Ok(bytes) = response.bytes().await else { return; };
                                if bytes.len() > 2_000_000 { return; }
                                let decoded = tokio::task::spawn_blocking(move || {
                                    let image = image::load_from_memory(&bytes).ok()?.thumbnail(320, 320).to_rgba8();
                                    Some(egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], &image))
                                }).await;
                                if let Ok(Some(image)) = decoded { send(&updates, &ctx, Response::Cover(url, image)); }
                            }
                        }
                    });
                    if is_resolve { resolver_job = Some(job); }
                    else if let Some(id) = search_id { search_jobs.insert(id, job); }
                }
            });
        });
        Self { tx, rx }
    }

    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }
}
