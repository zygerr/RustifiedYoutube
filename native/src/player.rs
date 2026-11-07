use eframe::egui;
use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use windows::{
    Foundation::{TimeSpan, TypedEventHandler, Uri},
    Media::{
        Core::MediaSource,
        Playback::{
            MediaPlaybackState, MediaPlayer, MediaPlayerAudioCategory, MediaPlayerFailedEventArgs,
        },
    },
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

pub enum PlayerCommand {
    Play {
        generation: u64,
        audio: rustified_core::resolver::PreparedAudio,
    },
    Toggle,
    Volume(f64),
    Seek(f64),
    Stop,
    Shutdown,
}

#[derive(Default, Clone, PartialEq)]
pub struct PlayerState {
    pub generation: u64,
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
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Player {
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, commands) = mpsc::channel();
        let (updates, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            if run(commands, &updates, &ctx).is_err() {
                let _ = updates.send(PlayerState {
                    error: Some(
                        "Windows audio could not start. Check your audio output device.".into(),
                    ),
                    ..Default::default()
                });
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            worker: Some(worker),
        }
    }
    pub fn send(&self, command: PlayerCommand) {
        let _ = self.tx.send(command);
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.send(PlayerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

enum MediaEvent {
    Ended(u64),
    Failed(u64, String),
}

fn create_player(
    generation: u64,
    volume: f64,
    events: Sender<MediaEvent>,
) -> windows::core::Result<MediaPlayer> {
    let player = MediaPlayer::new()?;
    player.SetAudioCategory(MediaPlayerAudioCategory::Media)?;
    player.SetIsMuted(false)?;
    player.SetVolume(volume)?;
    let ended = events.clone();
    player.MediaEnded(&TypedEventHandler::new(move |_, _| {
        let _ = ended.send(MediaEvent::Ended(generation));
        Ok(())
    }))?;
    player.MediaFailed(
        &TypedEventHandler::<MediaPlayer, MediaPlayerFailedEventArgs>::new(move |_, args| {
            let detail = args
                .as_ref()
                .and_then(|args| args.ExtendedErrorCode().ok())
                .map(|code| format!("Windows audio error {code:?}."))
                .unwrap_or_else(|| "Windows could not decode this track.".into());
            let _ = events.send(MediaEvent::Failed(generation, detail));
            Ok(())
        }),
    )?;
    Ok(player)
}

fn natural_end(last_position: f64, duration: f64) -> bool {
    last_position > 0.5 && duration > 0.0 && last_position >= duration - 3.0
}

fn run(
    commands: Receiver<PlayerCommand>,
    updates: &Sender<PlayerState>,
    ctx: &egui::Context,
) -> windows::core::Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let (events, notifications) = mpsc::channel();
    let mut volume = 0.5;
    let mut generation = 0;
    let mut player = create_player(generation, volume, events.clone())?;
    let mut current_audio = None;
    let mut active = false;
    let mut last_sent: Option<PlayerState> = None;
    let mut last_position = 0.0;
    let mut opened_at = Instant::now();
    loop {
        match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(PlayerCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(PlayerCommand::Play {
                generation: id,
                audio,
            }) => {
                // Each source gets its own event handlers. Late events from the
                // previous source retain its generation and cannot skip this song.
                let _ = player.Close();
                generation = id;
                player = create_player(generation, volume, events.clone())?;
                current_audio = Some(audio);
                last_position = 0.0;
                last_sent = None;
                opened_at = Instant::now();
                let result = (|| {
                    let audio = current_audio.as_ref().expect("current audio");
                    let source =
                        MediaSource::CreateFromUri(&Uri::CreateUri(&HSTRING::from(&audio.url))?)?;
                    player.SetSource(&source)?;
                    player.Play()
                })();
                active = true;
                if let Err(error) = result {
                    let _ = events.send(MediaEvent::Failed(
                        generation,
                        format!("Windows audio error {:?}.", error.code()),
                    ));
                }
            }
            Ok(PlayerCommand::Stop) => {
                active = false;
                generation = 0;
                let _ = player.Close();
                current_audio = None;
                player = create_player(generation, volume, events.clone())?;
            }
            Ok(PlayerCommand::Toggle) if active => {
                if player.PlaybackSession()?.PlaybackState()? == MediaPlaybackState::Playing {
                    player.Pause()?;
                } else {
                    player.Play()?;
                }
            }
            Ok(PlayerCommand::Volume(value)) => {
                volume = value.clamp(0.0, 1.0);
                player.SetVolume(volume)?;
            }
            Ok(PlayerCommand::Seek(seconds)) if active => {
                let seconds = seconds.max(0.0);
                player.PlaybackSession()?.SetPosition(TimeSpan {
                    Duration: (seconds * 10_000_000.0) as i64,
                })?;
                last_position = seconds;
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut ended = false;
        let mut error = None;
        while let Ok(event) = notifications.try_recv() {
            match event {
                MediaEvent::Ended(id) if id == generation && active => ended = true,
                MediaEvent::Failed(id, detail) if id == generation && active => {
                    error = Some(detail)
                }
                _ => {}
            }
        }
        if !active {
            continue;
        }
        let session = player.PlaybackSession()?;
        let state = session.PlaybackState()?;
        let position = session.Position()?.Duration as f64 / 10_000_000.0;
        let duration = session.NaturalDuration()?.Duration as f64 / 10_000_000.0;
        if ended && !natural_end(last_position, duration) {
            ended = false;
            error = Some("This track stopped before playback completed. Try playing it again or choose another result.".into());
        }
        let buffering =
            state == MediaPlaybackState::Buffering || state == MediaPlaybackState::Opening;
        if buffering && opened_at.elapsed() > Duration::from_secs(30) && position < 0.5 {
            error = Some(
                "Windows could not start this track within 30 seconds. Try playing it again."
                    .into(),
            );
        }
        let update = PlayerState {
            generation,
            position,
            duration,
            playing: state == MediaPlaybackState::Playing && error.is_none(),
            buffering: buffering && error.is_none(),
            ended,
            error,
        };
        if last_sent.as_ref() != Some(&update) {
            let _ = updates.send(update.clone());
            last_sent = Some(update.clone());
            ctx.request_repaint();
        }
        last_position = position;
        if update.ended || update.error.is_some() {
            active = false;
            let _ = player.Pause();
        }
    }
    let _ = player.Close();
    drop(player);
    drop(current_audio);
    unsafe {
        RoUninitialize();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn immediate_eof_does_not_advance_the_queue() {
        assert!(!super::natural_end(0.0, 207.57));
        assert!(!super::natural_end(1.0, 207.57));
        assert!(super::natural_end(207.2, 207.57));
    }
}
