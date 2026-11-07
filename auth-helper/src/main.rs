#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rustified_core::{browser_session, session};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

#[derive(Clone)]
enum LoginEvent {
    CheckSession(Option<u8>),
    Checked(bool),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoopBuilder::<LoginEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Continue with Google — RustifiedYoutube")
        .with_inner_size(tao::dpi::LogicalSize::new(960., 760.))
        .build(&event_loop)?;
    let mut context = WebContext::new(Some(session::profile_dir()?));
    let page_proxy = event_loop.create_proxy();
    let webview=WebViewBuilder::new_with_web_context(&mut context)
        .with_additional_browser_args("--autoplay-policy=no-user-gesture-required")
        .with_url(session::music_url(session::load_session().ok().flatten().and_then(|saved|saved.auth_user)))
        .with_initialization_script(r#"
            if (location.origin==='https://music.youtube.com') {
                let redirected=false;
                setInterval(()=>{
                    const signed=window.ytcfg?.get?.('LOGGED_IN');
                    if (signed===true && document.querySelector('ytmusic-player-bar')) {
                        const raw=window.ytcfg?.get?.('SESSION_INDEX');
                        const index=raw===null || raw===undefined?NaN:Number(raw);
                        window.ipc.postMessage(JSON.stringify({kind:'signed-in',auth_user:Number.isInteger(index)&&index>=0&&index<=255?index:null}));
                    } else if (signed===false && !redirected) {
                        const link=document.querySelector('ytmusic-nav-bar a[href^="https://accounts.google.com/"]');
                        if(link) {redirected=true;location.assign(link.href);}
                    }
                },1000);
            }
        "#)
        .with_ipc_handler(move|request| {
            if request.uri().scheme_str()==Some("https") && request.uri().host()==Some("music.youtube.com") && request.body().len()<=1024
                && let Ok(message)=serde_json::from_str::<serde_json::Value>(request.body()) && message["kind"]=="signed-in" {
                    let index=message["auth_user"].as_u64().and_then(|index|u8::try_from(index).ok());
                    let _=page_proxy.send_event(LoginEvent::CheckSession(index));
            }
        })
        .with_new_window_req_handler(|url,_|if url.starts_with("https://")||url=="about:blank" {wry::NewWindowResponse::Allow} else {wry::NewWindowResponse::Deny})
        .with_navigation_handler(|url|url.starts_with("https://")||url=="about:blank")
        .build(&window)?;
    let proxy = event_loop.create_proxy();
    let mut pending = false;
    let mut completed = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::LoopDestroyed => std::process::exit(if completed { 0 } else { 1 }),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(LoginEvent::Checked(success)) => {
                pending = false;
                if success {
                    completed = true;
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(LoginEvent::CheckSession(index)) if !pending => {
                if !session::native_enabled() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                pending = true;
                let proxy = proxy.clone();
                let result = (|| -> windows::core::Result<()> {
                    let core = unsafe { webview.controller().CoreWebView2()? };
                    browser_session::capture(&core, index, move |captured| {
                        let success = if session::native_enabled() {
                            captured
                                .ok()
                                .flatten()
                                .is_some_and(|saved| session::save_session(&saved).is_ok())
                        } else {
                            false
                        };
                        let _ = proxy.send_event(LoginEvent::Checked(success));
                    })
                })();
                if result.is_err() {
                    pending = false;
                }
            }
            _ => {}
        }
    });
}
