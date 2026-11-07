use anyhow::{Context, Result};
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

pub fn save_cookie(cookie: &str) -> Result<()> {
    anyhow::ensure!(cookie.contains("SAPISID="), "Sign-in has not completed yet");
    let encrypted = crypt(cookie.as_bytes(), true)?;
    let path = app_dir()?.join("session.dpapi");
    fs::write(path, encrypted)?;
    Ok(())
}

pub fn load_cookie() -> Result<Option<String>> {
    let path = app_dir()?.join("session.dpapi");
    if !path.exists() {
        return Ok(None);
    }
    let encrypted = fs::read(path)?;
    let plain = crypt(&encrypted, false)?;
    Ok(Some(String::from_utf8(plain)?))
}

pub fn disconnect() -> Result<()> {
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
