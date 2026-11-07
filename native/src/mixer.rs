//! Labels only audio sessions belonging to this player's dedicated WebView2
//! browser. It never writes volume, mute, output routing, or another app's labels.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, S_OK},
        Media::Audio::{
            AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2,
            IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
            eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                CoUninitialize,
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    },
    core::{HSTRING, Interface, PWSTR, Result},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Scope {
    pid: u32,
    created: u64,
}
fn created(pid: u32) -> Result<u64> {
    let handle = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? });
    let (mut birth, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle.0, &mut birth, &mut exit, &mut kernel, &mut user)?;
    }
    Ok((u64::from(birth.dwHighDateTime) << 32) | u64::from(birth.dwLowDateTime))
}

#[derive(Clone, Copy)]
struct Process {
    parent: u32,
    webview: bool,
}
fn processes() -> Result<HashMap<u32, Process>> {
    let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut result = HashMap::new();
    let mut next = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while next.is_ok() {
        let length = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        result.insert(
            entry.th32ProcessID,
            Process {
                parent: entry.th32ParentProcessID,
                webview: String::from_utf16_lossy(&entry.szExeFile[..length])
                    .eq_ignore_ascii_case("msedgewebview2.exe"),
            },
        );
        next = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    Ok(result)
}
fn owns(mut pid: u32, root: u32, processes: &HashMap<u32, Process>) -> bool {
    if root == 0 || !processes.get(&root).is_some_and(|p| p.webview) {
        return false;
    }
    let mut visited = HashSet::new();
    for _ in 0..32 {
        if pid == 0 || !visited.insert(pid) {
            return false;
        }
        let Some(process) = processes.get(&pid).filter(|process| process.webview) else {
            return false;
        };
        if pid == root {
            return true;
        }
        pid = process.parent;
    }
    false
}
fn take(value: PWSTR) -> Result<String> {
    if value.is_null() {
        return Ok(String::new());
    }
    let result = unsafe { value.to_string() };
    unsafe {
        CoTaskMemFree(Some(value.0.cast()));
    }
    Ok(result?)
}

enum Command {
    Scope(Scope),
    Stop,
}
pub struct Mixer {
    tx: mpsc::Sender<Command>,
    verified_sessions: Arc<AtomicUsize>,
    worker: Option<std::thread::JoinHandle<()>>,
}

fn icon_path() -> Option<std::path::PathBuf> {
    // Some Windows mixer surfaces accept the session name but fail to load
    // executable resource locators. Give them an actual ICO file instead.
    // Keep it in app data so installs, portable moves and upgrades don't leave
    // the audio service pointing at a removed executable or temporary file.
    let directory = rustified_core::session::app_dir().ok()?;
    let path = directory.join("mixer-icon-v1.ico");
    let bytes = include_bytes!("../../assets/icon.ico");
    if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

impl Mixer {
    pub fn start() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let standalone = icon_path();
        let source = standalone.as_deref().unwrap_or(&exe);
        // Preserve session branding even if the app-data icon cannot be written.
        let icon = standalone.as_ref().map_or_else(
            || format!("{},-101", exe.display()),
            |path| path.to_string_lossy().into_owned(),
        );
        let mut native_icon = windows::Win32::UI::WindowsAndMessaging::HICON::default();
        let mut small_icon = windows::Win32::UI::WindowsAndMessaging::HICON::default();
        let loaded = unsafe {
            windows::Win32::UI::Shell::ExtractIconExW(
                &HSTRING::from(source.to_string_lossy().as_ref()),
                if standalone.is_some() { 0 } else { -101 },
                Some(&mut native_icon),
                Some(&mut small_icon),
                1,
            )
        };
        let icon_valid = loaded > 0 && !native_icon.is_invalid() && !small_icon.is_invalid();
        for handle in [native_icon, small_icon] {
            if !handle.is_invalid() {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(handle);
                }
            }
        }
        if smoke_active() {
            println!(
                "Application icon available: {icon_valid}; standalone ICO: {}",
                standalone.is_some()
            );
        }
        let (tx, rx) = mpsc::channel();
        let verified_sessions = Arc::new(AtomicUsize::new(0));
        let verified = verified_sessions.clone();
        let worker = std::thread::Builder::new()
            .name("audio-identity".into())
            .spawn(move || {
                let result = run(rx, verified, icon, icon_valid);
                if smoke_active()
                    && let Err(error) = result
                {
                    eprintln!("Audio identity check failed: {error}");
                }
            })
            .ok()?;
        Some(Self {
            tx,
            verified_sessions,
            worker: Some(worker),
        })
    }
    pub fn set_browser(&self, pid: u32) {
        if pid != 0
            && let Ok(created) = created(pid)
        {
            let _ = self.tx.send(Command::Scope(Scope { pid, created }));
        }
    }
    pub fn verified(&self) -> bool {
        self.verified_sessions.load(Ordering::Relaxed) > 0
    }
}
impl Drop for Mixer {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn run(
    rx: mpsc::Receiver<Command>,
    verified: Arc<AtomicUsize>,
    icon: String,
    icon_valid: bool,
) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    let _apartment = Apartment;
    let devices: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let name = HSTRING::from("RustifiedYoutube");
    let icon_wide = HSTRING::from(&icon);
    let mut scope = None;
    // Keep references alive: Core Audio labels last only as long as the session
    // control object does. Refresh the list for source and output-device changes.
    let mut retained = HashMap::<String, IAudioSessionControl2>::new();
    let mut last_check = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Command::Scope(value)) => scope = Some(value),
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Ok(command) = rx.try_recv() {
            match command {
                Command::Scope(value) => scope = Some(value),
                Command::Stop => return Ok(()),
            }
        }
        let Some(scope) = scope else {
            continue;
        };
        // Creation time prevents a recycled browser PID from claiming a different app.
        if created(scope.pid).ok() != Some(scope.created) {
            retained.clear();
            verified.store(0, Ordering::Relaxed);
            continue;
        }
        let Ok(processes) = processes() else {
            continue;
        };
        let Ok(outputs) = (unsafe { devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) })
        else {
            continue;
        };
        let mut seen = HashSet::new();
        let mut verified_count = 0;
        let mut owned = 0;
        for index in 0..unsafe { outputs.GetCount().unwrap_or(0) } {
            let result = (|| -> Result<()> {
                let output = unsafe { outputs.Item(index)? };
                let manager: IAudioSessionManager2 = unsafe { output.Activate(CLSCTX_ALL, None)? };
                let sessions = unsafe { manager.GetSessionEnumerator()? };
                for index in 0..unsafe { sessions.GetCount()? } {
                    let _ = (|| -> Result<()> {
                        let session: IAudioSessionControl2 =
                            unsafe { sessions.GetSession(index)? }.cast()?;
                        if unsafe { session.IsSystemSoundsSession() } == S_OK
                            || unsafe { session.GetState()? } == AudioSessionStateExpired
                            || !owns(unsafe { session.GetProcessId()? }, scope.pid, &processes)
                        {
                            return Ok(());
                        }
                        owned += 1;
                        let key = take(unsafe { session.GetSessionInstanceIdentifier()? })?;
                        let volume = if smoke_active() {
                            session.cast::<ISimpleAudioVolume>().ok()
                        } else {
                            None
                        };
                        let before = volume.as_ref().and_then(|v| unsafe {
                            Some((v.GetMasterVolume().ok()?, v.GetMute().ok()?))
                        });
                        if take(unsafe { session.GetDisplayName()? })? != "RustifiedYoutube" {
                            unsafe {
                                session.SetDisplayName(&name, std::ptr::null())?;
                            }
                        }
                        if take(unsafe { session.GetIconPath()? })? != icon {
                            unsafe {
                                session.SetIconPath(&icon_wide, std::ptr::null())?;
                            }
                        }
                        let after = volume.as_ref().and_then(|v| unsafe {
                            Some((v.GetMasterVolume().ok()?, v.GetMute().ok()?))
                        });
                        let correct = take(unsafe { session.GetDisplayName()? })?
                            == "RustifiedYoutube"
                            && take(unsafe { session.GetIconPath()? })? == icon;
                        if correct
                            && icon_valid
                            && (!smoke_active() || (before.is_some() && before == after))
                        {
                            verified_count += 1;
                        }
                        retained.insert(key.clone(), session);
                        seen.insert(key);
                        Ok(())
                    })();
                }
                Ok(())
            })();
            let _ = result;
        }
        retained.retain(|key, _| seen.contains(key));
        verified.store(verified_count, Ordering::Relaxed);
        if smoke_active() && last_check != Some((owned, seen.len(), verified_count)) {
            println!(
                "Audio identity check: owned {owned}, labeled {}, verified sessions {verified_count}",
                seen.len()
            );
            last_check = Some((owned, seen.len(), verified_count));
        }
    }
    Ok(())
}
pub fn smoke_active() -> bool {
    std::env::args().any(|arg| arg == "--smoke-mixer")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_actual_webview_browser_and_its_webview_children_are_owned() {
        let processes = HashMap::from([
            (
                10,
                Process {
                    parent: 1,
                    webview: true,
                },
            ),
            (
                11,
                Process {
                    parent: 10,
                    webview: true,
                },
            ),
            (
                12,
                Process {
                    parent: 11,
                    webview: true,
                },
            ),
            (
                20,
                Process {
                    parent: 1,
                    webview: true,
                },
            ),
            (
                21,
                Process {
                    parent: 20,
                    webview: true,
                },
            ),
            (
                30,
                Process {
                    parent: 10,
                    webview: false,
                },
            ),
        ]);
        assert!(owns(10, 10, &processes));
        assert!(owns(12, 10, &processes));
        for pid in [0, 1, 20, 21, 30, 999] {
            assert!(!owns(pid, 10, &processes));
        }
        assert!(!owns(12, 0, &processes));
    }
    #[test]
    fn incomplete_or_cyclic_process_trees_cannot_claim_sessions() {
        let processes = HashMap::from([
            (
                10,
                Process {
                    parent: 1,
                    webview: true,
                },
            ),
            (
                11,
                Process {
                    parent: 12,
                    webview: true,
                },
            ),
            (
                12,
                Process {
                    parent: 11,
                    webview: true,
                },
            ),
        ]);
        assert!(!owns(11, 10, &processes));
        assert!(!owns(12, 999, &processes));
    }
}
