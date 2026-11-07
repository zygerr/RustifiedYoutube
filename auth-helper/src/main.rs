#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rustified_core::session;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use webview2_com::{GetCookiesCompletedHandler, Microsoft::Web::WebView2::Win32::ICoreWebView2_2};
use windows::{
    Win32::System::Com::CoTaskMemFree,
    core::{Interface, PWSTR, w},
};
use wry::{PageLoadEvent, WebContext, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

#[derive(Clone)]
enum LoginEvent {
    CheckSession,
    Checked(bool),
}

unsafe fn take_string(pointer: PWSTR) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let value = unsafe { pointer.to_string() }.unwrap_or_default();
    unsafe {
        CoTaskMemFree(Some(pointer.0.cast()));
    }
    value
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoopBuilder::<LoginEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Connect Google — RustifiedYoutube")
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 760.0))
        .build(&event_loop)?;
    let mut context = WebContext::new(Some(session::profile_dir()?));
    let page_proxy = event_loop.create_proxy();
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_additional_browser_args("--autoplay-policy=no-user-gesture-required")
        .with_url("https://music.youtube.com/")
        .with_new_window_req_handler(|url, _| {
            if url.starts_with("https://") || url == "about:blank" {
                wry::NewWindowResponse::Allow
            } else {
                wry::NewWindowResponse::Deny
            }
        })
        .with_navigation_handler(|url| url.starts_with("https://") || url == "about:blank")
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, PageLoadEvent::Finished)
                && url.starts_with("https://music.youtube.com/")
            {
                let _ = page_proxy.send_event(LoginEvent::CheckSession);
            }
        })
        .build(&window)?;
    let proxy = event_loop.create_proxy();
    let mut pending = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(LoginEvent::Checked(success)) => {
                pending = false;
                if success {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(LoginEvent::CheckSession) if !pending => {
                pending = true;
                let proxy = proxy.clone();
                let result = (|| -> windows::core::Result<()> {
                    let core: ICoreWebView2_2 =
                        unsafe { webview.controller().CoreWebView2()?.cast()? };
                    let manager = unsafe { core.CookieManager()? };
                    let callback =
                        GetCookiesCompletedHandler::create(Box::new(move |status, cookies| {
                            let mut values = Vec::new();
                            if status.is_ok()
                                && let Some(cookies) = cookies
                            {
                                let mut count = 0;
                                unsafe {
                                    cookies.Count(&mut count)?;
                                }
                                for index in 0..count {
                                    let cookie = unsafe { cookies.GetValueAtIndex(index)? };
                                    let (mut name, mut value) = (PWSTR::null(), PWSTR::null());
                                    unsafe {
                                        cookie.Name(&mut name)?;
                                        cookie.Value(&mut value)?;
                                    }
                                    let (name, value) =
                                        unsafe { (take_string(name), take_string(value)) };
                                    values.push(format!("{name}={value}"));
                                }
                            }
                            let success = session::save_cookie(&(values.join("; ") + ";")).is_ok();
                            let _ = proxy.send_event(LoginEvent::Checked(success));
                            Ok(())
                        }));
                    unsafe { manager.GetCookies(w!("https://music.youtube.com/"), &callback) }
                })();
                if result.is_err() {
                    pending = false;
                }
            }
            _ => {}
        }
    });
}
