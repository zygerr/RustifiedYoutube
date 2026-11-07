use crate::backend::Response;
use eframe::egui;
use rustified_core::resolver::{self, PreparedAudio};
use std::{collections::VecDeque, sync::mpsc};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
};

pub enum Request {
    Play(u64, String),
    Prefetch(Option<String>),
    Session(Option<String>),
}
enum Event {
    Status(u64, String),
    Finished(u64, Result<PreparedAudio, String>),
}
struct Job {
    ticket: u64,
    video: String,
    foreground: Option<u64>,
    task: JoinHandle<()>,
}

struct Cache<T> {
    entries: VecDeque<(String, T, u64)>,
    bytes: u64,
}
impl<T: Clone> Cache<T> {
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
        }
    }
    fn get(&mut self, id: &str) -> Option<T> {
        let index = self.entries.iter().position(|entry| entry.0 == id)?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }
    fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|entry| entry.0 == id)
    }
    fn insert(&mut self, id: String, value: T, bytes: u64) {
        if let Some(index) = self.entries.iter().position(|entry| entry.0 == id) {
            let old = self.entries.remove(index).expect("cache entry");
            self.bytes -= old.2;
        }
        self.entries.push_back((id, value, bytes));
        self.bytes += bytes;
        while self.entries.len() > 3 || self.bytes > 160_000_000 {
            if let Some((_, _, bytes)) = self.entries.pop_front() {
                self.bytes -= bytes;
            }
        }
    }
}

fn emit(tx: &mpsc::Sender<Response>, ctx: &egui::Context, response: Response) {
    if tx.send(response).is_ok() {
        ctx.request_repaint();
    }
}
fn start(
    ticket: &mut u64,
    video: String,
    foreground: Option<u64>,
    cookie: &Option<String>,
    events: &UnboundedSender<Event>,
) -> Job {
    *ticket += 1;
    let id = *ticket;
    let task_video = video.clone();
    let cookie = cookie.clone();
    let events = events.clone();
    let task = tokio::spawn(async move {
        let result = resolver::prepare_with_status(&task_video, cookie.as_deref(), |status| {
            let _ = events.send(Event::Status(id, status.into()));
        })
        .await
        .map_err(|error| error.to_string());
        let _ = events.send(Event::Finished(id, result));
    });
    Job {
        ticket: id,
        video,
        foreground,
        task,
    }
}

pub fn spawn(
    cookie: Option<String>,
    updates: mpsc::Sender<Response>,
    ctx: egui::Context,
) -> UnboundedSender<Request> {
    let (tx, rx) = unbounded_channel();
    tokio::spawn(run(rx, cookie, updates, ctx));
    tx
}
async fn run(
    mut requests: UnboundedReceiver<Request>,
    mut cookie: Option<String>,
    updates: mpsc::Sender<Response>,
    ctx: egui::Context,
) {
    let (events, mut notifications) = unbounded_channel();
    let mut cache = Cache::new();
    let mut job: Option<Job> = None;
    let mut next = None;
    let mut ticket = 0;
    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else { break; };
                match request {
                    Request::Play(id,video) => {
                        // A foreground selection always wins over unrelated prefetch work.
                        if let Some(audio) = cache.get(&video) {
                            if job.as_ref().is_some_and(|job|job.foreground.is_some() && job.video != video) {
                                job.take().expect("obsolete foreground job").task.abort();
                            }
                            emit(&updates,&ctx,Response::Stream(id,Ok(audio)));
                        } else {
                            if job.as_ref().is_some_and(|job|job.video != video) {
                                job.take().expect("running job").task.abort();
                            }
                            if let Some(job) = &mut job {
                                job.foreground = Some(id);
                                emit(&updates,&ctx,Response::Preparing(id,"Finishing preloaded audio…".into()));
                            } else { job = Some(start(&mut ticket,video,Some(id),&cookie,&events)); }
                        }
                    }
                    Request::Prefetch(video) => {
                        if job.as_ref().is_some_and(|job|job.foreground.is_none() && Some(&job.video) != video.as_ref()) {
                            job.take().expect("prefetch job").task.abort();
                        }
                        next = video;
                    }
                    Request::Session(value) => {
                        if let Some(job) = job.take() { job.task.abort(); }
                        cache = Cache::new();
                        next = None;
                        cookie = value;
                    }
                }
            }
            event = notifications.recv() => {
                match event {
                    Some(Event::Status(id,status)) => {
                        if let Some(job) = &job && job.ticket == id && let Some(request) = job.foreground {
                            emit(&updates,&ctx,Response::Preparing(request,status));
                        }
                    }
                    Some(Event::Finished(id,result)) if job.as_ref().is_some_and(|job|job.ticket == id) => {
                        let finished = job.take().expect("finished job");
                        match result {
                            Ok(audio) => {
                                let bytes = audio.bytes;
                                if let Some(request) = finished.foreground { emit(&updates,&ctx,Response::Stream(request,Ok(audio.clone()))); }
                                else { emit(&updates,&ctx,Response::Preloaded(finished.video.clone())); }
                                cache.insert(finished.video,audio,bytes);
                            }
                            Err(error) => if let Some(request) = finished.foreground { emit(&updates,&ctx,Response::Stream(request,Err(error))); }
                        }
                    }
                    _ => {}
                }
            }
        }
        if job.is_none()
            && let Some(video) = next.take()
            && !cache.contains(&video)
        {
            job = Some(start(&mut ticket, video, None, &cookie, &events));
        }
    }
    if let Some(job) = job {
        job.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;
    #[test]
    fn cache_keeps_recent_tracks_and_respects_the_byte_limit() {
        let mut cache = Cache::new();
        for id in ["a", "b", "c"] {
            cache.insert(id.into(), id, 10);
        }
        assert_eq!(cache.get("a"), Some("a"));
        cache.insert("d".into(), "d", 10);
        assert!(!cache.contains("b"));
        cache.insert("large".into(), "large", 160_000_000);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, 160_000_000);
    }
    #[test]
    fn replacing_a_track_does_not_leak_cache_budget() {
        let mut cache = Cache::new();
        cache.insert("a".into(), 1, 100);
        cache.insert("a".into(), 2, 25);
        assert_eq!(cache.bytes, 25);
        assert_eq!(cache.get("a"), Some(2));
    }
}
