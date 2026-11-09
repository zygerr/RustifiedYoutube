//! Windows thumbnail transport controls. Shell notifications are captured before
//! window creation, including sent messages that bypass winit's queued-message hook.
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::GetCurrentThreadId,
        },
        UI::{
            Shell::{
                DefSubclassProc, ITaskbarList3, RemoveWindowSubclass, SetWindowSubclass, THB_FLAGS,
                THB_ICON, THB_TOOLTIP, THBF_DISABLED, THBF_ENABLED, THBF_HIDDEN, THBN_CLICKED,
                THUMBBUTTON, TaskbarList,
            },
            WindowsAndMessaging::{
                CWPSTRUCT, CallNextHookEx, CreateIconFromResourceEx, DestroyIcon, HHOOK, HICON,
                LR_DEFAULTCOLOR, RegisterWindowMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
                WH_CALLWNDPROC, WM_COMMAND, WM_NCDESTROY,
            },
        },
    },
    core::{PCWSTR, w},
};

const PREVIOUS: u32 = 0xB510;
const TOGGLE: u32 = 0xB511;
const NEXT: u32 = 0xB512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Previous,
    Toggle,
    Next,
}
impl Action {
    fn id(self) -> u32 {
        match self {
            Self::Previous => PREVIOUS,
            Self::Toggle => TOGGLE,
            Self::Next => NEXT,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Controls {
    pub has_track: bool,
    pub playing: bool,
    pub can_next: bool,
}
impl Controls {
    fn enabled(self, action: Action) -> bool {
        self.has_track && (action != Action::Next || self.can_next)
    }
}

enum Event {
    Created(usize),
    Click(usize, Action),
}
struct HookState {
    created_message: u32,
    tx: mpsc::Sender<Event>,
    wake: mpsc::SyncSender<()>,
}
thread_local! {static HOOK:RefCell<Option<HookState>>=const {RefCell::new(None)};}

fn command(message: u32, wparam: usize, lparam: isize) -> Option<Action> {
    if message != WM_COMMAND || lparam != 0 || ((wparam >> 16) & 0xFFFF) != THBN_CLICKED as usize {
        return None;
    }
    match (wparam & 0xFFFF) as u32 {
        PREVIOUS => Some(Action::Previous),
        TOGGLE => Some(Action::Toggle),
        NEXT => Some(Action::Next),
        _ => None,
    }
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam.0 != 0 {
        // WH_CALLWNDPROC guarantees a CWPSTRUCT for the duration of this call.
        let msg = unsafe { &*(lparam.0 as *const CWPSTRUCT) };
        signal(msg.hwnd, msg.message, msg.wParam, msg.lParam);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn signal(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let wake = HOOK.with(|slot| {
        let state = slot.borrow();
        let state = state.as_ref()?;
        let event = if message == state.created_message {
            Some(Event::Created(hwnd.0 as usize))
        } else {
            command(message, wparam.0, lparam.0).map(|action| Event::Click(hwnd.0 as usize, action))
        };
        if let Some(event) = event {
            let _ = state.tx.send(event);
            Some(state.wake.clone())
        } else {
            None
        }
    });
    // Window creation can run while egui holds its Context lock. Wake via
    // a worker instead of entering egui synchronously from a Win32 hook.
    if let Some(wake) = wake {
        let _ = wake.try_send(());
    }
}
const SUBCLASS_ID: usize = 0x52595442;
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    signal(hwnd, message, wparam, lparam);
    if message == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub struct Events {
    rx: mpsc::Receiver<Event>,
    hook: HHOOK,
    window: Option<HWND>,
    wake_context: Arc<Mutex<Option<egui::Context>>>,
    _waker: std::thread::JoinHandle<()>,
}
impl Events {
    /// Called on the GUI thread before eframe creates any windows.
    pub fn start() -> windows::core::Result<Self> {
        let message = unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) };
        if message == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let (tx, rx) = mpsc::channel();
        let hook_handle =
            unsafe { SetWindowsHookExW(WH_CALLWNDPROC, Some(hook), None, GetCurrentThreadId())? };
        let (wake, wake_rx) = mpsc::sync_channel::<()>(1);
        let wake_context = Arc::new(Mutex::new(None::<egui::Context>));
        let worker_context = wake_context.clone();
        let waker = std::thread::Builder::new()
            .name("taskbar-wake".into())
            .spawn(move || {
                while wake_rx.recv().is_ok() {
                    let context = worker_context.lock().ok().and_then(|slot| slot.clone());
                    if let Some(context) = context {
                        context.request_repaint();
                    }
                }
            })
            .map_err(|error| {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook_handle);
                }
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    error.to_string(),
                )
            })?;
        HOOK.with(|slot| {
            *slot.borrow_mut() = Some(HookState {
                created_message: message,
                tx,
                wake,
            })
        });
        if smoke_active() {
            println!("Taskbar message hook registered");
        }
        Ok(Self {
            rx,
            hook: hook_handle,
            window: None,
            wake_context,
            _waker: waker,
        })
    }
}
impl Events {
    fn bind(&mut self, hwnd: HWND, ctx: egui::Context) -> windows::core::Result<()> {
        if !unsafe { SetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, 0) }.as_bool() {
            return Err(windows::core::Error::from_thread());
        }
        self.window = Some(hwnd);
        // From here, observe only the root window. Its subclass receives both
        // posted and sent messages, without intercepting WebView child windows.
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
        self.hook = HHOOK::default();
        if let Ok(mut slot) = self.wake_context.lock() {
            *slot = Some(ctx);
        }
        Ok(())
    }
}
impl Drop for Events {
    fn drop(&mut self) {
        unsafe {
            if !self.hook.is_invalid() {
                let _ = UnhookWindowsHookEx(self.hook);
            }
            if let Some(hwnd) = self.window {
                let _ = RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
            }
        }
        HOOK.with(|slot| *slot.borrow_mut() = None);
    }
}

struct Apartment;
impl Apartment {
    fn start() -> windows::core::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

struct Icon(HICON);
impl Icon {
    fn new(shape: Glyph) -> windows::core::Result<Self> {
        let data = icon_resource(shape);
        unsafe { CreateIconFromResourceEx(&data, true, 0x30000, 32, 32, LR_DEFAULTCOLOR).map(Self) }
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}
#[derive(Clone, Copy)]
enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
}

fn inside(glyph: Glyph, x: f32, y: f32) -> bool {
    match glyph {
        Glyph::Play => (10. ..=24.).contains(&x) && (y - 16.).abs() <= (24. - x) * 0.65,
        Glyph::Pause => {
            (7. ..=25.).contains(&y) && ((9. ..=13.).contains(&x) || (19. ..=23.).contains(&x))
        }
        Glyph::Next => {
            (6. ..=20.).contains(&x) && (y - 16.).abs() <= (20. - x) * 0.65
                || ((23. ..=26.).contains(&x) && (7. ..=25.).contains(&y))
        }
        Glyph::Previous => inside(Glyph::Next, 32. - x, y),
    }
}

/// A 32-bit DIB icon resource: white glyphs with a subtle dark outline stay
/// readable on both light and dark Windows taskbar previews. No disk assets.
fn icon_resource(shape: Glyph) -> Vec<u8> {
    const SIZE: usize = 32;
    let mut data = Vec::with_capacity(40 + SIZE * SIZE * 4 + SIZE * 4);
    for value in [40u32, 32, 64] {
        data.extend(value.to_le_bytes());
    }
    data.extend(1u16.to_le_bytes());
    data.extend(32u16.to_le_bytes());
    for value in [0u32, (SIZE * SIZE * 4) as u32, 0, 0, 0, 0] {
        data.extend(value.to_le_bytes());
    }
    let mut mask = [0u8; SIZE * 4];
    for row in 0..SIZE {
        for x in 0..SIZE {
            let mut fill = 0.;
            let mut outline = 0.;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) / 4.;
                    let py = (SIZE - 1 - row) as f32 + (sy as f32 + 0.5) / 4.;
                    if inside(shape, px, py) {
                        fill += 1.;
                    } else if [(-0.8, 0.), (0.8, 0.), (0., -0.8), (0., 0.8)]
                        .iter()
                        .any(|(dx, dy)| inside(shape, px + dx, py + dy))
                    {
                        outline += 1.;
                    }
                }
            }
            let alpha = ((fill * 255. + outline * 220.) / 16.) as u8;
            let rgb = ((fill * 245. + outline * 22.) / 16.) as u8;
            data.extend([rgb, rgb, rgb, alpha]);
            if alpha == 0 {
                mask[row * 4 + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    data.extend(mask);
    data
}

fn wide_tip(value: &str) -> [u16; 260] {
    let mut result = [0u16; 260];
    let mut count = 0;
    for character in value.chars().filter(|c| *c != '\0') {
        let mut units = [0; 2];
        let units = character.encode_utf16(&mut units);
        if count + units.len() >= result.len() {
            break;
        }
        result[count..count + units.len()].copy_from_slice(units);
        count += units.len();
    }
    result
}

pub struct Taskbar {
    events: Events,
    hwnd: HWND,
    ctx: egui::Context,
    shell: Option<ITaskbarList3>,
    icons: [Icon; 4],
    ready: bool,
    installed: bool,
    controls: Controls,
    last_controls: Option<Controls>,
    tooltip: [u16; 260],
    last_tooltip: Option<[u16; 260]>,
    attempts: u8,
    last_attempt: Option<Instant>,
    _apartment: Apartment,
}
impl Taskbar {
    pub fn start(cc: &eframe::CreationContext<'_>, events: Option<Events>) -> Option<Self> {
        if crate::preview::active() {
            return None;
        }
        let mut events = events?;
        if smoke_active() {
            println!(
                "Taskbar binding playback window; Explorer desktop present: {}",
                unsafe { !windows::Win32::UI::WindowsAndMessaging::GetShellWindow().is_invalid() }
            );
        }
        let RawWindowHandle::Win32(handle) = cc.window_handle().ok()?.as_raw() else {
            return None;
        };
        let apartment = Apartment::start()
            .map_err(|error| {
                if smoke_active() {
                    eprintln!("Taskbar COM initialization failed: {error}");
                }
                error
            })
            .ok()?;
        let icons = [
            Icon::new(Glyph::Previous).ok()?,
            Icon::new(Glyph::Play).ok()?,
            Icon::new(Glyph::Pause).ok()?,
            Icon::new(Glyph::Next).ok()?,
        ];
        events
            .bind(HWND(handle.hwnd.get() as *mut _), cc.egui_ctx.clone())
            .ok()?;
        if smoke_active() {
            println!("Taskbar window bound; visible: {}", unsafe {
                windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(HWND(
                    handle.hwnd.get() as *mut _
                ))
                .as_bool()
            });
        }
        Some(Self {
            events,
            hwnd: HWND(handle.hwnd.get() as *mut _),
            ctx: cc.egui_ctx.clone(),
            shell: None,
            icons,
            ready: false,
            installed: false,
            controls: Controls::default(),
            last_controls: None,
            tooltip: wide_tip("RustifiedYoutube"),
            last_tooltip: None,
            attempts: 0,
            last_attempt: None,
            _apartment: apartment,
        })
    }

    pub fn poll(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Ok(event) = self.events.rx.try_recv() {
            match event {
                Event::Created(hwnd) if hwnd == self.hwnd.0 as usize => {
                    if smoke_active() {
                        println!("Windows created the taskbar button");
                    }
                    self.ready = true;
                    self.installed = false;
                    self.last_controls = None;
                    self.last_tooltip = None;
                    self.shell = None;
                    self.attempts = 0;
                    self.last_attempt = None;
                }
                Event::Click(hwnd, action)
                    if hwnd == self.hwnd.0 as usize && self.controls.enabled(action) =>
                {
                    actions.push(action)
                }
                _ => {}
            }
        }
        actions
    }

    fn buttons(&self, hidden: bool) -> [THUMBBUTTON; 3] {
        [Action::Previous, Action::Toggle, Action::Next].map(|action| {
            let (icon, tip) = match action {
                Action::Previous => (0, "Previous track"),
                Action::Next => (3, "Next track"),
                Action::Toggle if self.controls.playing => (2, "Pause"),
                Action::Toggle => (1, "Play"),
            };
            THUMBBUTTON {
                dwMask: THB_FLAGS | THB_ICON | THB_TOOLTIP,
                iId: action.id(),
                hIcon: self.icons[icon].0,
                szTip: wide_tip(tip),
                dwFlags: if hidden {
                    THBF_HIDDEN
                } else if self.controls.enabled(action) {
                    THBF_ENABLED
                } else {
                    THBF_DISABLED
                },
                ..Default::default()
            }
        })
    }

    pub fn update(&mut self, controls: Controls, title: &str) {
        self.controls = controls;
        self.tooltip = wide_tip(title);
        if !self.ready {
            return;
        }
        if !self.installed {
            if self.attempts >= 5
                || self
                    .last_attempt
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
            {
                return;
            }
            self.attempts += 1;
            self.last_attempt = Some(Instant::now());
            let result = (|| -> windows::core::Result<()> {
                let shell: ITaskbarList3 =
                    unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)? };
                unsafe {
                    shell.HrInit()?;
                    shell.ThumbBarAddButtons(self.hwnd, &self.buttons(false))?;
                }
                self.shell = Some(shell);
                Ok(())
            })();
            if let Err(error) = result {
                if smoke_active() {
                    eprintln!("Taskbar toolbar registration failed: {error}");
                }
                self.ctx.request_repaint_after(Duration::from_secs(1));
                return;
            }
            self.installed = true;
            self.last_controls = Some(controls);
            if smoke_active() {
                println!("Windows taskbar thumbnail toolbar registered");
            }
        }
        if let Some(shell) = &self.shell {
            if self.last_controls != Some(controls)
                && unsafe { shell.ThumbBarUpdateButtons(self.hwnd, &self.buttons(false)) }.is_ok()
            {
                self.last_controls = Some(controls);
            }
            if self.last_tooltip != Some(self.tooltip)
                && unsafe { shell.SetThumbnailTooltip(self.hwnd, PCWSTR(self.tooltip.as_ptr())) }
                    .is_ok()
            {
                self.last_tooltip = Some(self.tooltip);
            }
        }
    }

    pub fn smoke_show(&self) {
        if (smoke_active() || crate::account::smoke_active())
            && !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(self.hwnd) }
                .as_bool()
        {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    self.hwnd,
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
                );
            }
        }
    }

    pub fn synchronized(&self, controls: Controls) -> bool {
        self.installed && self.last_controls == Some(controls)
    }

    pub fn installed(&self) -> bool {
        self.installed
    }

    /// Sends the same notification used by Explorer, only to this app's window.
    pub fn smoke_click(&self, action: Action) {
        if smoke_active() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(self.hwnd),
                    WM_COMMAND,
                    WPARAM(((THBN_CLICKED as usize) << 16) | action.id() as usize),
                    LPARAM(0),
                );
            }
        }
    }
}
impl Drop for Taskbar {
    fn drop(&mut self) {
        if self.installed
            && let Some(shell) = &self.shell
        {
            unsafe {
                let _ = shell.ThumbBarUpdateButtons(self.hwnd, &self.buttons(true));
            }
        }
        // Release COM before balancing this instance's CoInitializeEx call.
        self.shell = None;
    }
}
pub fn smoke_active() -> bool {
    std::env::args().any(|arg| arg == "--smoke-taskbar")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_thumbnail_click_notifications_for_our_ids_are_actions() {
        assert_eq!(
            command(
                WM_COMMAND,
                ((THBN_CLICKED as usize) << 16) | TOGGLE as usize,
                0
            ),
            Some(Action::Toggle)
        );
        for (message, wparam, lparam) in [
            (WM_COMMAND, TOGGLE as usize, 0),
            (
                WM_COMMAND,
                ((THBN_CLICKED as usize) << 16) | TOGGLE as usize,
                1,
            ),
            (WM_COMMAND, ((THBN_CLICKED as usize) << 16) | 7, 0),
            (0, ((THBN_CLICKED as usize) << 16) | TOGGLE as usize, 0),
        ] {
            assert_eq!(command(message, wparam, lparam), None);
        }
        let idle = Controls::default();
        assert!(!idle.enabled(Action::Toggle));
        assert!(!idle.enabled(Action::Previous));
        let last = Controls {
            has_track: true,
            playing: true,
            can_next: false,
        };
        assert!(last.enabled(Action::Toggle));
        assert!(!last.enabled(Action::Next));
    }
    #[test]
    fn tooltips_preserve_complete_utf16_characters_and_always_terminate() {
        let tip = wide_tip(&("a".repeat(258) + "🚀"));
        let length = tip.iter().position(|unit| *unit == 0).unwrap();
        assert_eq!(length, 258);
        assert!(String::from_utf16(&tip[..length]).is_ok());
        assert_eq!(wide_tip("Play\0pause")[4], b'p' as u16);
    }
    #[test]
    fn generated_icons_are_valid_native_resources() {
        for shape in [Glyph::Previous, Glyph::Play, Glyph::Pause, Glyph::Next] {
            assert_eq!(icon_resource(shape).len(), 40 + 32 * 32 * 4 + 32 * 4);
            assert!(!Icon::new(shape).unwrap().0.is_invalid());
        }
    }
}
