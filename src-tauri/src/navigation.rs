use tauri::Url;

fn is_https(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
}

/// A reload or server redirect must never launch the system browser. Google
/// can redirect through regional account hosts when completing sign-in.
/// These pages stay in the unprivileged WebView, just like ordinary HTTPS
/// navigation in a browser. Native capabilities remain disabled for all of them.
pub fn allow_navigation(url: &Url) -> bool {
    is_https(url) || url.as_str() == "about:blank"
}

/// Google/YouTube popups need a real WebView window and its opener relationship.
/// Treating them as a navigation of the main window breaks authentication.
pub fn is_internal(url: &Url) -> bool {
    is_https(url)
        && matches!(
            url.host_str(),
            Some(
                "music.youtube.com"
                    | "accounts.google.com"
                    | "consent.google.com"
                    | "consent.youtube.com"
                    | "www.youtube.com"
                    | "youtube.com"
                    | "accounts.youtube.com"
                    | "www.google.com"
                    | "myaccount.google.com"
                    | "google.ro"
                    | "www.google.ro"
                    | "accounts.google.ro"
            )
        )
}

#[derive(Debug, PartialEq, Eq)]
pub enum PopupRoute {
    WebView,
    Browser,
    Block,
}

pub fn popup_route(url: &Url) -> PopupRoute {
    if is_internal(url) || url.as_str() == "about:blank" {
        PopupRoute::WebView
    } else if is_https(url) {
        PopupRoute::Browser
    } else {
        PopupRoute::Block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_and_google_login_stay_in_the_app() {
        for value in [
            "https://music.youtube.com/watch?v=example",
            "https://accounts.google.com/ServiceLogin",
            "https://consent.youtube.com/",
            "https://accounts.youtube.com/accounts/SetSID",
            "https://www.google.com/",
        ] {
            assert!(is_internal(&Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn similar_hosts_and_non_https_urls_are_not_internal() {
        for value in [
            "https://music.youtube.com.example.org/",
            "https://youtube.com@example.org/",
            "http://music.youtube.com/",
            "https://music.youtube.com:8443/",
            "file:///C:/Windows/win.ini",
            "javascript:alert(1)",
        ] {
            assert!(!is_internal(&Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn only_https_can_launch_the_browser() {
        assert_eq!(
            popup_route(&Url::parse("https://github.com/zygerr/RustifiedYoutube").unwrap()),
            PopupRoute::Browser
        );
        for value in [
            "file:///C:/test",
            "mailto:someone@example.org",
            "javascript:alert(1)",
        ] {
            assert_eq!(popup_route(&Url::parse(value).unwrap()), PopupRoute::Block);
        }
    }

    #[test]
    fn account_completion_redirects_and_reloads_remain_navigations() {
        for value in [
            "https://accounts.youtube.com/accounts/SetSID",
            "https://accounts.google.ro/",
            "https://www.google.com/accounts/CheckCookie",
            "https://accounts.google.co.uk/",
            "https://music.youtube.com/",
        ] {
            assert!(allow_navigation(&Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn auth_popups_preserve_the_webview_context_including_blank_openers() {
        for value in [
            "about:blank",
            "https://accounts.google.com/ServiceLogin",
            "https://accounts.youtube.com/accounts/SetSID",
            "https://www.google.com/accounts/CheckCookie",
            "https://music.youtube.com/",
        ] {
            assert_eq!(
                popup_route(&Url::parse(value).unwrap()),
                PopupRoute::WebView
            );
        }
    }

    #[test]
    fn insecure_or_local_navigation_is_not_accepted() {
        for value in [
            "file:///C:/Windows/win.ini",
            "http://accounts.google.com/",
            "javascript:alert(1)",
            "https://user:password@accounts.google.com/",
        ] {
            assert!(!allow_navigation(&Url::parse(value).unwrap()));
        }
    }
}
