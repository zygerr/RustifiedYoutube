use crate::backend::Response;
use anyhow::{Context, Result};
use rustified_core::{
    api::{AuthRejected, Catalog, SessionExpired, is_auth_error},
    session::{self, SavedSession},
};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::{Mutex, RwLock, watch};

pub fn smoke_active() -> bool {
    std::env::args().any(|arg| {
        matches!(
            arg.as_str(),
            "--smoke-account" | "--smoke-player-session" | "--smoke-login" | "--smoke-sign-out"
        )
    })
}

struct State {
    enabled: bool,
    signing_out: bool,
    enabled_before_sign_out: Option<bool>,
    saved: Option<SavedSession>,
    client: Option<Arc<Catalog>>,
    revision: u64,
}
pub struct Account {
    state: RwLock<State>,
    bootstrap: Mutex<()>,
    refresh: Mutex<()>,
    captures: watch::Sender<Option<Capture>>,
    next_capture: AtomicU64,
    control_epoch: AtomicU64,
    reported: AtomicBool,
    responses: mpsc::Sender<Response>,
    ctx: eframe::egui::Context,
}
impl Account {
    pub fn new(responses: mpsc::Sender<Response>, ctx: eframe::egui::Context) -> Self {
        let enabled = session::native_enabled();
        let saved = if enabled {
            session::load_session().ok().flatten()
        } else {
            None
        };
        let connected = saved.is_some();
        let (captures, _) = watch::channel(None);
        let account = Self {
            state: RwLock::new(State {
                enabled,
                signing_out: false,
                enabled_before_sign_out: None,
                saved,
                client: None,
                revision: 0,
            }),
            bootstrap: Mutex::new(()),
            refresh: Mutex::new(()),
            captures,
            next_capture: AtomicU64::new(1),
            control_epoch: AtomicU64::new(0),
            reported: AtomicBool::new(connected),
            responses,
            ctx,
        };
        account.send(Response::Ready(connected));
        account.status(if connected {
            "Google session saved · checking connection…"
        } else if enabled {
            "Checking the saved browser account…"
        } else {
            "Native library disconnected"
        });
        account
    }
    fn send(&self, response: Response) {
        if self.responses.send(response).is_ok() {
            self.ctx.request_repaint();
        }
    }
    fn status(&self, message: &str) {
        self.send(Response::AccountStatus(message.into()));
    }
    fn connected(&self, value: bool) {
        if self.reported.swap(value, Ordering::SeqCst) != value {
            self.send(Response::Ready(value));
        }
    }
    async fn store(&self, mut saved: SavedSession) -> Result<bool> {
        let mut state = self.state.write().await;
        if !state.enabled || state.signing_out {
            return Ok(false);
        }
        if saved.auth_user.is_none() {
            saved.auth_user = state.saved.as_ref().and_then(|saved| saved.auth_user);
        }
        if saved.user_agent.is_none() {
            saved.user_agent = state
                .saved
                .as_ref()
                .and_then(|saved| saved.user_agent.clone());
        }
        if state.saved.as_ref() != Some(&saved) {
            session::save_session(&saved)?;
            state.saved = Some(saved);
            state.client = None;
            state.revision += 1;
        }
        // A capture comes from the authenticated playback document. Library
        // requests can recover independently once the browser session is saved.
        self.connected(true);
        Ok(true)
    }
    async fn client(&self) -> Result<(u64, Arc<Catalog>)> {
        let _bootstrap = self.bootstrap.lock().await;
        loop {
            let (revision, saved) = {
                let state = self.state.read().await;
                if let Some(client) = &state.client {
                    return Ok((state.revision, client.clone()));
                }
                (
                    state.revision,
                    if state.enabled {
                        state.saved.clone()
                    } else {
                        None
                    },
                )
            };
            let result = match &saved {
                Some(saved) => Catalog::from_session(saved).await,
                None => Catalog::new(None).await,
            };
            let mut state = self.state.write().await;
            if state.revision != revision {
                continue;
            }
            let client = Arc::new(result?);
            state.client = Some(client.clone());
            if client.signed_in() {
                self.connected(true);
                self.status("Google account connected · browser session synchronized");
            }
            return Ok((revision, client));
        }
    }
    async fn capture(&self) -> Result<Option<SavedSession>> {
        let mut captures = self.captures.subscribe();
        let id = self.next_capture.fetch_add(1, Ordering::SeqCst);
        self.send(Response::CaptureSession(id));
        let capture = tokio::time::timeout(
            Duration::from_secs(8),
            captures.wait_for(|capture| capture.as_ref().is_some_and(|capture| capture.id == id)),
        )
        .await
        .context("Browser session refresh timed out")?
        .context("Browser session channel closed")?
        .clone()
        .context("Browser session missing")?;
        capture
            .result
            .map_err(|_| anyhow::anyhow!("Browser session could not be read yet"))
    }
    pub async fn captured(&self, capture: Capture) {
        if capture.id != 0 {
            self.captures.send_replace(Some(capture));
            return;
        }
        if !self.state.read().await.enabled {
            return;
        }
        let empty = matches!(capture.result, Ok(None));
        if let Ok(Some(saved)) = capture.result {
            match self.store(saved).await {
                Ok(true) => {}
                Ok(false) => return,
                Err(_) => {
                    self.status("Couldn't save the Google session. Try refreshing the connection in Settings.");
                    return;
                }
            }
            match self.client().await {
                Ok(_) => {}
                Err(error) if is_auth_error(&error) => self.status(
                    "Library connection interrupted. Retrying with the saved Google account…",
                ),
                Err(_) => {
                    self.status("Google session saved · waiting for the connection to return")
                }
            }
        } else if empty && self.state.read().await.saved.is_none() {
            self.status("Not connected · use Continue with Google to open your library");
        }
    }
    async fn recover(&self, failed_revision: u64) -> Result<()> {
        let _refresh = self.refresh.lock().await;
        {
            let state = self.state.read().await;
            if !state.enabled {
                return Err(SessionExpired.into());
            }
            if state.revision != failed_revision
                && state
                    .client
                    .as_ref()
                    .is_some_and(|client| client.signed_in())
            {
                return Ok(());
            }
        }
        let epoch = self.control_epoch.load(Ordering::SeqCst);
        self.status("Refreshing the Google session automatically…");
        // An empty cookie snapshot can occur while the WebView is navigating
        // or its cookie store is initializing. It cannot establish that Google
        // signed out; only an authenticated bootstrap may report expiry.
        let saved = self.capture().await?.ok_or(AuthRejected)?;
        if self.control_epoch.load(Ordering::SeqCst) != epoch {
            return Err(SessionExpired.into());
        }
        self.store(saved).await?;
        // A rejected cached client must be bootstrapped again even if the
        // cookie header is unchanged; YouTube's client configuration can rotate.
        {
            let mut state = self.state.write().await;
            state.client = None;
            state.revision += 1;
        }
        let (_, client) = self.client().await?;
        if !client.signed_in() {
            return Err(SessionExpired.into());
        }
        self.connected(true);
        self.status("Google account connected · session refreshed");
        Ok(())
    }
    pub async fn execute<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn(Arc<Catalog>) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let epoch = self.control_epoch.load(Ordering::SeqCst);
        anyhow::ensure!(
            !self.state.read().await.signing_out,
            "Google sign-out is in progress"
        );
        let mut revision = self.state.read().await.revision;
        let first = match self.client().await {
            Ok((version, client)) => {
                revision = version;
                operation(client).await
            }
            Err(error) => Err(error),
        };
        if self.control_epoch.load(Ordering::SeqCst) != epoch {
            anyhow::bail!("Account connection changed");
        }
        if first.as_ref().is_err_and(is_auth_error) {
            let recovered = self.recover(revision).await;
            let result = match recovered {
                Ok(()) => match self.client().await {
                    Ok((_, client)) => operation(client).await,
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            };
            if self.control_epoch.load(Ordering::SeqCst) != epoch {
                anyhow::bail!("Account connection changed");
            }
            if result.as_ref().is_err_and(is_auth_error) {
                self.status("Library connection interrupted. Your saved account will be retried.");
            } else if result.is_err() {
                self.status("Google session saved · check your internet connection");
            }
            result
        } else {
            first
        }
    }
    pub async fn execute_public<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn(Arc<Catalog>) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let result = self.execute(&operation).await;
        if result.as_ref().is_err_and(is_auth_error) {
            operation(Arc::new(Catalog::new(None).await?)).await
        } else {
            result
        }
    }
    pub async fn connect(&self, force_login: bool) -> Result<bool> {
        let _refresh = self.refresh.lock().await;
        anyhow::ensure!(
            !self.state.read().await.signing_out,
            "Google sign-out is in progress"
        );
        session::enable_native()?;
        let epoch = {
            let mut state = self.state.write().await;
            anyhow::ensure!(!state.signing_out, "Google sign-out is in progress");
            if !state.enabled {
                state.enabled = true;
                self.control_epoch.fetch_add(1, Ordering::SeqCst) + 1
            } else {
                self.control_epoch.load(Ordering::SeqCst)
            }
        };
        self.status("Reusing the Google account saved in this app…");
        if force_login {
            self.status("Complete Google sign-in in the account panel.");
            return Ok(false);
        }
        let captured = self.capture().await?;
        anyhow::ensure!(
            self.control_epoch.load(Ordering::SeqCst) == epoch,
            "Account connection changed"
        );
        if let Some(saved) = captured {
            self.store(saved).await?;
        }
        {
            // A refresh must not open the sign-in panel as a side effect.
            let mut state = self.state.write().await;
            anyhow::ensure!(state.saved.is_some(), AuthRejected);
            state.client = None;
            state.revision += 1;
        }
        let (_, client) = self.client().await?;
        anyhow::ensure!(client.signed_in(), AuthRejected);
        anyhow::ensure!(
            self.control_epoch.load(Ordering::SeqCst) == epoch,
            "Account connection changed"
        );
        self.connected(true);
        self.status("Google account connected · browser session synchronized");
        Ok(true)
    }
    pub async fn disconnect(&self) -> Result<()> {
        let _refresh = self.refresh.lock().await;
        self.control_epoch.fetch_add(1, Ordering::SeqCst);
        let mut state = self.state.write().await;
        anyhow::ensure!(!state.signing_out, "Google sign-out is in progress");
        let result = session::disconnect();
        state.enabled = false;
        state.saved = None;
        state.client = None;
        state.revision += 1;
        drop(state);
        self.connected(false);
        self.status("Native library disconnected · the browser account is kept");
        result
    }

    pub async fn begin_google_sign_out(&self) -> Result<()> {
        let _refresh = self.refresh.lock().await;
        let mut state = self.state.write().await;
        anyhow::ensure!(!state.signing_out, "Google sign-out is already in progress");
        self.control_epoch.fetch_add(1, Ordering::SeqCst);
        state.enabled_before_sign_out = Some(state.enabled);
        state.enabled = false;
        state.signing_out = true;
        state.client = None;
        state.revision += 1;
        drop(state);
        self.status("Signing out of Google in this app…");
        Ok(())
    }

    pub async fn finish_google_sign_out(&self) -> Result<()> {
        let _refresh = self.refresh.lock().await;
        let mut state = self.state.write().await;
        anyhow::ensure!(state.signing_out, "Google sign-out is not in progress");
        session::disconnect()?;
        state.enabled = false;
        state.signing_out = false;
        state.enabled_before_sign_out = None;
        state.saved = None;
        state.client = None;
        state.revision += 1;
        drop(state);
        self.connected(false);
        self.status("Google signed out of this app");
        Ok(())
    }

    pub async fn cancel_google_sign_out(&self, message: &str) {
        let _refresh = self.refresh.lock().await;
        let mut state = self.state.write().await;
        if state.signing_out {
            let was_enabled = state.enabled_before_sign_out.take().unwrap_or(false);
            state.enabled = was_enabled && session::native_enabled();
            state.signing_out = false;
            state.client = None;
            state.revision += 1;
            self.control_epoch.fetch_add(1, Ordering::SeqCst);
        }
        drop(state);
        self.status(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(enabled: bool) -> (Arc<Account>, mpsc::Receiver<Response>) {
        let (responses, receiver) = mpsc::channel();
        let (captures, _) = watch::channel(None);
        let saved = SavedSession {
            cookies: "SAPISID=fixture;".into(),
            user_agent: Some("Fixture browser".into()),
            auth_user: None,
        };
        (
            Arc::new(Account {
                state: RwLock::new(State {
                    enabled,
                    signing_out: false,
                    enabled_before_sign_out: None,
                    saved: Some(saved),
                    client: None,
                    revision: 0,
                }),
                bootstrap: Mutex::new(()),
                refresh: Mutex::new(()),
                captures,
                next_capture: AtomicU64::new(5),
                control_epoch: AtomicU64::new(0),
                reported: AtomicBool::new(enabled),
                responses,
                ctx: Default::default(),
            }),
            receiver,
        )
    }
    async fn requested(receiver: &mpsc::Receiver<Response>) -> u64 {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(Response::CaptureSession(id)) = receiver.try_recv() {
                    return id;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn disabled_accounts_ignore_background_credentials_without_touching_disk() {
        let (account, responses) = fixture(false);
        account
            .captured(Capture {
                id: 0,
                result: Ok(Some(SavedSession {
                    cookies: "SAPISID=other;".into(),
                    user_agent: None,
                    auth_user: None,
                })),
            })
            .await;
        assert!(!account.reported.load(Ordering::SeqCst));
        assert_eq!(
            account.state.read().await.saved.as_ref().unwrap().cookies,
            "SAPISID=fixture;"
        );
        assert!(responses.try_recv().is_err());
    }
    #[tokio::test]
    async fn an_old_capture_cannot_complete_a_new_refresh_request() {
        let (account, responses) = fixture(true);
        let worker = account.clone();
        let task = tokio::spawn(async move { worker.capture().await });
        let id = requested(&responses).await;
        account
            .captured(Capture {
                id: id - 1,
                result: Ok(None),
            })
            .await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        account
            .captured(Capture {
                id,
                result: Ok(Some(SavedSession {
                    cookies: "SAPISID=fresh;".into(),
                    user_agent: None,
                    auth_user: None,
                })),
            })
            .await;
        let saved = task.await.unwrap().unwrap().unwrap();
        assert_eq!(saved.cookies, "SAPISID=fresh;");
    }
    #[tokio::test]
    async fn cookie_read_failure_keeps_the_saved_account_and_credentials() {
        let (account, responses) = fixture(true);
        let worker = account.clone();
        let task = tokio::spawn(async move { worker.capture().await });
        let id = requested(&responses).await;
        account
            .captured(Capture {
                id,
                result: Err(()),
            })
            .await;
        assert!(task.await.unwrap().is_err());
        assert!(account.reported.load(Ordering::SeqCst));
        assert_eq!(
            account.state.read().await.saved.as_ref().unwrap().cookies,
            "SAPISID=fixture;"
        );
    }

    #[tokio::test]
    async fn an_empty_refresh_snapshot_does_not_report_a_saved_account_signed_out() {
        let (account, responses) = fixture(true);
        let worker = account.clone();
        let task = tokio::spawn(async move { worker.recover(0).await });
        let id = requested(&responses).await;
        account
            .captured(Capture {
                id,
                result: Ok(None),
            })
            .await;
        let error = task.await.unwrap().unwrap_err();
        assert!(is_auth_error(&error));
        assert!(!rustified_core::api::is_expired_error(&error));
        assert!(account.reported.load(Ordering::SeqCst));
        assert_eq!(
            account.state.read().await.saved.as_ref().unwrap().cookies,
            "SAPISID=fixture;"
        );
        assert!(
            !responses
                .try_iter()
                .any(|response| matches!(response, Response::Ready(false)))
        );
    }

    #[tokio::test]
    async fn sign_out_freezes_captures_until_confirmed_or_cancelled() {
        let (account, responses) = fixture(true);
        account.begin_google_sign_out().await.unwrap();
        account
            .captured(Capture {
                id: 0,
                result: Ok(Some(SavedSession {
                    cookies: "SAPISID=stale;".into(),
                    user_agent: None,
                    auth_user: None,
                })),
            })
            .await;
        {
            let state = account.state.read().await;
            assert!(!state.enabled);
            assert!(state.signing_out);
            assert_eq!(state.saved.as_ref().unwrap().cookies, "SAPISID=fixture;");
        }
        assert!(responses.try_recv().is_ok());

        account
            .cancel_google_sign_out("Google sign-out could not be confirmed")
            .await;
        let state = account.state.read().await;
        assert!(state.enabled);
        assert!(!state.signing_out);
        assert_eq!(state.saved.as_ref().unwrap().cookies, "SAPISID=fixture;");
    }
}

// Credentials travel only through native memory and DPAPI, never JS IPC or logs.
#[derive(Clone)]
pub struct Capture {
    pub id: u64,
    pub result: Result<Option<SavedSession>, ()>,
}
