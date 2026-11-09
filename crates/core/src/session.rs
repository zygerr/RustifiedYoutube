use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    },
    core::w,
};

pub fn app_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new().context("User data directory unavailable")?;
    let path = base.data_local_dir().join(crate::APP_ID);
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn profile_dir() -> Result<PathBuf> {
    Ok(app_dir()?.join("webview"))
}

fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into()?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI binds the session to the current Windows user. It never leaves this PC.
    unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                w!("RustifiedYoutube session"),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(result)
    }
}

// Deliberately no Debug implementation: credentials must never enter logs.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedSession {
    pub cookies: String,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub auth_user: Option<u8>,
}
impl SavedSession {
    pub fn valid(&self) -> bool {
        authentication_cookie(&self.cookies).is_some()
    }
}

/// YouTube can rotate away SAPISID while keeping its secure third-party
/// equivalent. Use the same credential for validation and request signing so
/// a valid browser session is not discarded merely because its cookie changed.
pub fn authentication_cookie(cookies: &str) -> Option<&str> {
    ["SAPISID", "__Secure-3PAPISID"]
        .into_iter()
        .find_map(|name| {
            cookies.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key == name && !value.is_empty()).then_some(value)
            })
        })
}
pub fn save_session(session: &SavedSession) -> Result<()> {
    anyhow::ensure!(session.valid(), "Sign-in has not completed yet");
    let encrypted = crypt(&serde_json::to_vec(session)?, true)?;
    let directory = app_dir()?;
    let temporary = directory.join(format!("session-{}.dpapi.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)?;
    std::io::Write::write_all(&mut file, &encrypted)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, directory.join("session.dpapi"))?;
    Ok(())
}
pub fn load_session() -> Result<Option<SavedSession>> {
    let path = app_dir()?.join("session.dpapi");
    if !path.exists() {
        return Ok(None);
    }
    let encrypted = fs::read(path)?;
    let plain = crypt(&encrypted, false)?;
    let text = String::from_utf8(plain)?;
    Ok(Some(decode_session(&text)?))
}
fn decode_session(text: &str) -> Result<SavedSession> {
    let session = if text.trim_start().starts_with('{') {
        serde_json::from_str(text)?
    } else {
        SavedSession {
            cookies: text.into(),
            user_agent: None,
            auth_user: None,
        }
    };
    anyhow::ensure!(session.valid(), "Saved session is incomplete");
    Ok(session)
}
pub fn load_cookie() -> Result<Option<String>> {
    Ok(load_session()?.map(|session| session.cookies))
}
pub fn save_cookie(cookie: &str) -> Result<()> {
    save_session(&SavedSession {
        cookies: cookie.into(),
        user_agent: None,
        auth_user: None,
    })
}
pub fn native_enabled() -> bool {
    app_dir().is_ok_and(|directory| !directory.join("session.disabled").exists())
}
pub fn music_url(auth_user: Option<u8>) -> String {
    match auth_user {
        Some(index) => format!("https://music.youtube.com/?authuser={index}"),
        None => "https://music.youtube.com/".into(),
    }
}
pub fn enable_native() -> Result<()> {
    let marker = app_dir()?.join("session.disabled");
    if marker.exists() {
        fs::remove_file(marker)?;
    }
    Ok(())
}

pub fn disconnect() -> Result<()> {
    fs::write(
        app_dir()?.join("session.disabled"),
        b"Native library connection disabled by user",
    )?;
    let path = app_dir()?.join("session.dpapi");
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn adjacent_tool(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent().map(|p| {
                let adjacent = p.join(name);
                if adjacent.is_file() {
                    adjacent
                } else {
                    p.parent().map(|p| p.join(name)).unwrap_or(adjacent)
                }
            })
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_credentials_migrate_and_empty_or_similar_cookie_names_do_not_sign_in() {
        let saved = super::decode_session("SAPISID=fixture; SID=test;").unwrap();
        assert!(saved.valid());
        assert!(saved.user_agent.is_none());
        let encoded = serde_json::to_string(&saved).unwrap();
        assert!(super::decode_session(&encoded).unwrap() == saved);
        for cookie in [
            "SAPISID=;",
            "NOT_SAPISID=fixture;",
            "VISITOR_INFO1_LIVE=test;",
        ] {
            assert!(super::decode_session(cookie).is_err());
        }
    }
    #[test]
    fn selected_account_survives_serialization_and_legacy_json_migrates() {
        let saved = super::decode_session(
            r#"{"cookies":"SAPISID=fixture;","user_agent":null,"auth_user":2}"#,
        )
        .unwrap();
        assert_eq!(saved.auth_user, Some(2));
        assert_eq!(
            super::music_url(saved.auth_user),
            "https://music.youtube.com/?authuser=2"
        );
        let legacy =
            super::decode_session(r#"{"cookies":"SAPISID=fixture;","user_agent":null}"#).unwrap();
        assert_eq!(legacy.auth_user, None);
        assert_eq!(super::music_url(None), "https://music.youtube.com/");
        assert_eq!(
            super::decode_session(&serde_json::to_string(&saved).unwrap())
                .unwrap()
                .auth_user,
            Some(2)
        );
    }
    #[test]
    fn rotating_to_the_secure_authentication_cookie_keeps_the_session_usable() {
        let secure = "__Secure-3PAPISID=rotated; LOGIN_INFO=fixture;";
        assert!(super::decode_session(secure).unwrap().valid());
        assert_eq!(super::authentication_cookie(secure), Some("rotated"));
        assert_eq!(
            super::authentication_cookie("SAPISID=; __Secure-3PAPISID=rotated;"),
            Some("rotated")
        );
        assert_eq!(
            super::authentication_cookie("__Secure-3PAPISID=rotated; SAPISID=primary;"),
            Some("primary")
        );
        for cookies in [
            "NOT_SAPISID=wrong;",
            "NOT___Secure-3PAPISID=wrong;",
            "__Secure-3PAPISID=;",
        ] {
            assert!(super::authentication_cookie(cookies).is_none());
        }
    }
    #[test]
    fn encrypted_session_roundtrips_without_writing_user_profile() {
        let plain = br#"{"cookies":"SAPISID=fixture;","user_agent":"Fixture browser"}"#;
        let encrypted = super::crypt(plain, true).unwrap();
        assert_ne!(encrypted, plain);
        assert_eq!(super::crypt(&encrypted, false).unwrap(), plain);
    }
}
