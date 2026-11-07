use eframe::egui;
use rustified_core::model::{Playlist, SearchFilter, Track};
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    time::Duration,
};
use tokio::sync::Semaphore;

pub enum Request {
    Search(u64, String, SearchFilter),
    Playlist(u64, String),
    Library,
    History,
    Discover,
    Connect { force_login: bool },
    Captured(crate::account::Capture),
    Disconnect,
    Like(String, bool),
    Cover(String, bool),
}

pub enum Response {
    Ready(bool),
    CaptureSession(u64),
    AccountStatus(String),
    ConnectFinished { browser_login: bool },
    Tracks(u64, Result<Vec<Track>, String>),
    Library(Result<Vec<Playlist>, String>),
    History(Result<Vec<Track>, String>),
    Discover(Result<Vec<Track>, String>),
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
        if crate::preview::active() {
            return Self { tx, rx };
        }
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("background runtime");
            runtime.block_on(async move {
                let account=Arc::new(crate::account::Account::new(updates.clone(),ctx.clone()));
                let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("image client");
                let images = Arc::new(Semaphore::new(4));
                let mut search_jobs: HashMap<u64, tokio::task::JoinHandle<()>> = HashMap::new();
                while let Some(request) = requests.recv().await {
                    let account = account.clone();
                    let updates = updates.clone();
                    let ctx = ctx.clone();
                    let http = http.clone();
                    let images = images.clone();
                    let search_id = match &request { Request::Search(id, _, _) | Request::Playlist(id, _) => Some(*id), _ => None };
                    if search_id.is_some() {
                        for (_, job) in search_jobs.drain() { job.abort(); }
                    }
                    let job = tokio::spawn(async move {
                        match request {
                            Request::Discover => {
                                let result=account.execute_public(|client|async move {client.search("popular music").await}).await.map_err(|_|"Couldn't load Discover. Check your connection and try refreshing.".into());
                                send(&updates,&ctx,Response::Discover(result));
                            }
                            Request::Search(id, query, filter) => {
                                let result=account.execute_public(|client|{let query=query.clone();async move {client.search_filtered(&query,filter).await}}).await.map_err(|_|"Search couldn't load. Check your connection and try again.".into());
                                send(&updates, &ctx, Response::Tracks(id, result));
                            }
                            Request::Playlist(id, playlist) => {
                                let result=account.execute(|client|{let playlist=playlist.clone();async move {client.playlist(&playlist).await}}).await.map_err(|_|"Couldn't load this playlist. Check your connection and try again.".into());
                                send(&updates, &ctx, Response::Tracks(id, result));
                            }
                            Request::Library => {
                                let result=account.execute(|client|async move {client.library().await}).await.map_err(|_|"Your library couldn't load. Check your connection; your saved account is kept.".into());
                                send(&updates, &ctx, Response::Library(result));
                            }
                            Request::History => {
                                let result=account.execute(|client|async move {client.history().await}).await.map_err(|_|"Couldn't refresh listening history. Your saved session and recent tracks are kept.".into());
                                send(&updates,&ctx,Response::History(result));
                            }
                            Request::Like(id, liked) => {
                                let success=account.execute(|client|{let id=id.clone();async move {client.like(&id,liked).await}}).await.is_ok();
                                send(&updates, &ctx, Response::Liked(id, liked, success));
                            }
                            Request::Connect {force_login} => {
                                let result=account.connect(force_login).await;
                                if result.is_err() {send(&updates,&ctx,Response::AccountStatus("Connection couldn't finish. Your saved account is kept; try again when online, or complete Google sign-in if prompted.".into()));}
                                else {send(&updates,&ctx,Response::Ready(true));}
                                send(&updates,&ctx,Response::ConnectFinished {browser_login:result.unwrap_or(false)});
                            }
                            Request::Captured(capture)=>account.captured(capture).await,
                            Request::Disconnect => {
                                if account.disconnect().await.is_err(){send(&updates,&ctx,Response::Notice("Couldn't disconnect the native library session. Try again.".into()));}
                            }
                            Request::Cover(url,large) => {
                                let Ok(_permit) = images.acquire().await else { return; };
                                if let Some(image)=crate::artwork::fetch(&http,&url,large).await {send(&updates,&ctx,Response::Cover(crate::artwork::key(&url,large),image));}
                            }
                        }
                    });
                    if let Some(id) = search_id { search_jobs.insert(id, job); }
                }
            });
        });
        Self { tx, rx }
    }

    pub fn send(&self, request: Request) {
        let _ = self.tx.send(request);
    }
}
