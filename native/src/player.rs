use eframe::egui;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};
use windows::{
    Foundation::{TimeSpan, TypedEventHandler, Uri},
    Media::{
        Core::MediaSource,
        Playback::{MediaPlaybackState, MediaPlayer, MediaPlayerFailedEventArgs},
    },
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

pub enum PlayerCommand {
    Play(rustified_core::resolver::PreparedAudio),
    Toggle,
    Volume(f64),
    Seek(f64),
    Stop,
    Shutdown,
}

#[derive(Default, Clone, PartialEq)]
pub struct PlayerState {
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub buffering: bool,
    pub ended: bool,
    pub error: Option<String>,
}

pub struct Player {
    pub tx: Sender<PlayerCommand>,
    pub rx: Receiver<PlayerState>,
}

impl Player {
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, commands) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run(commands, &updates, &ctx);
            if result.is_err() {
                let _ = updates.send(PlayerState {
                    error: Some(
                        "Windows audio could not start. Check your audio output device.".into(),
                    ),
                    ..Default::default()
                });
                ctx.request_repaint();
            }
        });
        Self { tx, rx }
    }
    pub fn send(&self, command: PlayerCommand) {
        let _ = self.tx.send(command);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.send(PlayerCommand::Shutdown);
    }
}

fn run(
    commands: Receiver<PlayerCommand>,
    updates: &Sender<PlayerState>,
    ctx: &egui::Context,
) -> windows::core::Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let player = MediaPlayer::new()?;
    player.SetVolume(0.5)?;
    let ended = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let failure_detail = Arc::new(Mutex::new(String::new()));
    let signal = ended.clone();
    let _ended_token = player.MediaEnded(&TypedEventHandler::new(move |_, _| {
        signal.store(true, Ordering::SeqCst);
        Ok(())
    }))?;
    let signal = failed.clone();
    let failure_code = failure_detail.clone();
    let _failed_token = player.MediaFailed(&TypedEventHandler::<
        MediaPlayer,
        MediaPlayerFailedEventArgs,
    >::new(move |_, args| {
        if let Some(args) = args.as_ref()
            && let Ok(mut detail) = failure_code.lock()
        {
            *detail = format!(
                "Audio error {:?}, {:?}",
                args.Error(),
                args.ExtendedErrorCode()
            );
        }
        signal.store(true, Ordering::SeqCst);
        Ok(())
    }))?;
    let mut active = false;
    let mut has_source = false;
    let mut current_audio = None;
    let mut last_sent: Option<PlayerState> = None;
    loop {
        match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(PlayerCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(PlayerCommand::Play(audio)) => {
                ended.store(false, Ordering::SeqCst);
                failed.store(false, Ordering::SeqCst);
                let result = (|| {
                    let source =
                        MediaSource::CreateFromUri(&Uri::CreateUri(&HSTRING::from(&audio.url))?)?;
                    player.SetSource(&source)?;
                    player.Play()
                })();
                active = result.is_ok();
                has_source = result.is_ok();
                current_audio = Some(audio);
                if result.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
            }
            Ok(PlayerCommand::Toggle) => {
                if !has_source {
                    continue;
                }
                if player.PlaybackSession()?.PlaybackState()? == MediaPlaybackState::Playing {
                    player.Pause()?;
                } else {
                    player.Play()?;
                }
            }
            Ok(PlayerCommand::Volume(value)) => player.SetVolume(value.clamp(0.0, 1.0))?,
            Ok(PlayerCommand::Seek(seconds)) => {
                if !has_source {
                    continue;
                }
                player.PlaybackSession()?.SetPosition(TimeSpan {
                    Duration: (seconds.max(0.0) * 10_000_000.0) as i64,
                })?
            }
            Ok(PlayerCommand::Stop) => {
                let _ = player.Pause();
                active = false;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if active || failed.load(Ordering::SeqCst) {
            let session = player.PlaybackSession()?;
            let state = session.PlaybackState()?;
            let is_ended = ended.swap(false, Ordering::SeqCst);
            let is_failed = failed.swap(false, Ordering::SeqCst);
            let update = PlayerState {
                position: session.Position()?.Duration as f64 / 10_000_000.0,
                duration: session.NaturalDuration()?.Duration as f64 / 10_000_000.0,
                playing: state == MediaPlaybackState::Playing,
                buffering: state == MediaPlaybackState::Buffering
                    || state == MediaPlaybackState::Opening,
                ended: is_ended,
                error: is_failed.then(|| {
                    format!(
                        "This stream could not be played. {}",
                        failure_detail.lock().map(|s| s.clone()).unwrap_or_default()
                    )
                }),
            };
            // A paused player does not need to repaint the GPU window every tick.
            if last_sent.as_ref() != Some(&update) {
                let _ = updates.send(update.clone());
                last_sent = Some(update);
                ctx.request_repaint();
            }
            if is_ended || is_failed {
                active = false;
            }
        }
    }
    let _ = player.Pause();
    drop(player);
    drop(current_audio);
    unsafe {
        RoUninitialize();
    }
    Ok(())
}
