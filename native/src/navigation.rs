use reqwest::Url;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Music,
    Account,
}

pub fn destination(value: &str) -> Option<Destination> {
    let url = Url::parse(value).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    match url.host_str()? {
        "music.youtube.com" => Some(Destination::Music),
        "accounts.google.com"
        | "myaccount.google.com"
        | "consent.google.com"
        | "accounts.youtube.com"
        | "consent.youtube.com" => Some(Destination::Account),
        "www.youtube.com" | "youtube.com" if matches!(url.path(), "/signin" | "/signin_prompt") => {
            Some(Destination::Account)
        }
        _ => None,
    }
}

pub fn sign_in_url() -> String {
    let mut handoff = Url::parse("https://www.youtube.com/signin").expect("fixed URL");
    handoff
        .query_pairs_mut()
        .append_pair("action_handle_signin", "true")
        .append_pair("app", "desktop")
        .append_pair("next", "https://music.youtube.com/");
    let mut url = Url::parse("https://accounts.google.com/ServiceLogin").expect("fixed URL");
    url.query_pairs_mut()
        .append_pair("ltmpl", "music")
        .append_pair("service", "youtube")
        .append_pair("continue", handoff.as_str());
    url.into()
}

pub fn sign_out_url() -> String {
    let mut url = Url::parse("https://accounts.google.com/Logout").expect("fixed URL");
    url.query_pairs_mut()
        .append_pair("continue", "https://music.youtube.com/");
    url.into()
}

pub fn watch_url(video_id: &str, auth_user: Option<u8>) -> Option<String> {
    if !rustified_core::model::valid_video_id(video_id) {
        return None;
    }
    let mut url = Url::parse("https://music.youtube.com/watch").ok()?;
    url.query_pairs_mut()
        .append_pair("v", video_id)
        .append_pair("list", &format!("RDAMVM{video_id}"));
    if let Some(index) = auth_user {
        url.query_pairs_mut()
            .append_pair("authuser", &index.to_string());
    }
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_official_https_account_handoffs_are_allowed() {
        for url in [
            "https://accounts.google.com/ServiceLogin",
            "https://accounts.youtube.com/accounts/CheckConnection",
            "https://consent.youtube.com/",
            "https://www.youtube.com/signin?next=music",
        ] {
            assert!(destination(url) == Some(Destination::Account));
        }
        for url in [
            "http://accounts.google.com/",
            "https://accounts.google.com.evil.test/",
            "https://accounts.google.com@evil.test/",
            "https://accounts.google.com:444/",
            "https://user:pass@accounts.google.com/",
            "https://www.youtube.com/watch?v=Fixture0001",
            "file:///test",
            "javascript:alert(1)",
        ] {
            assert!(destination(url).is_none());
        }
    }
    #[test]
    fn sign_out_uses_google_logout_and_returns_to_music() {
        let logout = Url::parse(&sign_out_url()).unwrap();
        assert!(destination(logout.as_str()) == Some(Destination::Account));
        assert_eq!(logout.host_str(), Some("accounts.google.com"));
        assert_eq!(logout.path(), "/Logout");
        assert_eq!(
            logout
                .query_pairs()
                .find(|(key, _)| key == "continue")
                .unwrap()
                .1,
            "https://music.youtube.com/"
        );
    }
    #[test]
    fn recovery_urls_keep_only_the_requested_track_and_selected_account() {
        let url = Url::parse(&watch_url("Fixture0001", Some(2)).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("music.youtube.com"));
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "authuser")
                .unwrap()
                .1,
            "2"
        );
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "v").unwrap().1,
            "Fixture0001"
        );
        assert!(watch_url("https://evil.test", None).is_none());
        let login = Url::parse(&sign_in_url()).unwrap();
        assert_eq!(login.host_str(), Some("accounts.google.com"));
        let handoff = login
            .query_pairs()
            .find(|(key, _)| key == "continue")
            .unwrap()
            .1
            .into_owned();
        assert_eq!(Url::parse(&handoff).unwrap().path(), "/signin");
    }
}
