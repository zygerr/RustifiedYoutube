//! Native, asynchronous access to this application's WebView2 profile only.
use crate::session::SavedSession;
use webview2_com::{
    GetCookiesCompletedHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2_2, ICoreWebView2Settings2},
};
use windows::{
    Win32::System::Com::CoTaskMemFree,
    core::{Interface, PWSTR, w},
};

unsafe fn take(pointer: PWSTR) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let text = unsafe { pointer.to_string() }.unwrap_or_default();
    unsafe {
        CoTaskMemFree(Some(pointer.0.cast()));
    }
    text
}

pub fn capture(
    core: &ICoreWebView2,
    auth_user: Option<u8>,
    complete: impl FnOnce(Result<Option<SavedSession>, ()>) + 'static,
) -> windows::core::Result<()> {
    let core: ICoreWebView2_2 = core.cast()?;
    let user_agent = unsafe {
        core.Settings()
            .and_then(|settings| settings.cast::<ICoreWebView2Settings2>())
            .and_then(|settings| {
                let mut agent = PWSTR::null();
                settings.UserAgent(&mut agent)?;
                Ok(take(agent))
            })
            .ok()
            .filter(|agent| !agent.is_empty())
    };
    let callback = GetCookiesCompletedHandler::create(Box::new(move |status, cookies| {
        let result = (|| -> windows::core::Result<Option<SavedSession>> {
            status?;
            let mut values = Vec::new();
            if let Some(cookies) = cookies {
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
                    values.push(unsafe { format!("{}={}", take(name), take(value)) });
                }
            }
            let session = SavedSession {
                cookies: values.join("; ") + ";",
                user_agent,
                auth_user,
            };
            Ok(session.valid().then_some(session))
        })();
        complete(result.map_err(|_| ()));
        Ok(())
    }));
    unsafe {
        core.CookieManager()?
            .GetCookies(w!("https://music.youtube.com/"), &callback)
    }
}
