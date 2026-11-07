use eframe::egui;
use std::io::Cursor;

pub const BROWSE_EDGE: u32 = 512;
pub const PLAYER_EDGE: u32 = 1536;
pub const CACHE_BYTES: usize = 64 * 1024 * 1024;
pub fn key(url: &str, large: bool) -> String {
    if large {
        format!("hd:{url}")
    } else {
        url.into()
    }
}

pub fn candidates(url: &str, edge: u32) -> Vec<String> {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return Vec::new();
    };
    let Some(host) = parsed.host_str() else {
        return Vec::new();
    };
    if parsed.scheme() != "https"
        || !(host.ends_with(".ytimg.com")
            || host.ends_with(".ggpht.com")
            || host.ends_with(".googleusercontent.com"))
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    if host.ends_with(".googleusercontent.com") || host.ends_with(".ggpht.com") {
        if let Some((base, options)) = parsed.path().rsplit_once('=') {
            let mut changed = false;
            let options = options
                .split('-')
                .map(|option| {
                    if option.len() > 1
                        && matches!(option.as_bytes()[0], b'w' | b'h' | b's')
                        && option[1..].bytes().all(|byte| byte.is_ascii_digit())
                    {
                        changed = true;
                        format!("{}{edge}", &option[..1])
                    } else {
                        option.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("-");
            if changed {
                let path = format!("{base}={options}");
                parsed.set_path(&path);
                result.push(parsed.to_string());
            }
        }
    } else if host == "i.ytimg.com" {
        let segments = parsed
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        if segments.len() == 3
            && matches!(segments[0], "vi" | "vi_webp")
            && rustified_core::model::valid_video_id(segments[1])
        {
            for file in ["maxresdefault.jpg", "sddefault.jpg"] {
                result.push(format!("https://i.ytimg.com/vi/{}/{file}", segments[1]));
            }
        }
    }
    if !result.iter().any(|candidate| candidate == url) {
        result.push(url.into());
    }
    result
}

fn decode(bytes: &[u8], edge: u32) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let source = reader.decode().ok()?;
    let source = if source.width() > edge || source.height() > edge {
        source.thumbnail(edge, edge)
    } else {
        source
    };
    let image = source.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        &image,
    ))
}
pub async fn fetch(client: &reqwest::Client, url: &str, large: bool) -> Option<egui::ColorImage> {
    let edge = if large { PLAYER_EDGE } else { BROWSE_EDGE };
    for candidate in candidates(url, edge) {
        let Ok(response) = client.get(&candidate).send().await else {
            continue;
        };
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|size| size > 8_000_000)
        {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if bytes.len() > 8_000_000 {
            continue;
        }
        let decoded = tokio::task::spawn_blocking(move || decode(&bytes, edge))
            .await
            .ok()
            .flatten();
        if let Some(image) = decoded {
            // Missing YouTube HD artwork may be a tiny placeholder even with 200.
            if candidate != url && candidate.contains("i.ytimg.com/vi/") && image.width() < 480 {
                continue;
            }
            return Some(image);
        }
    }
    None
}

pub fn smoke() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let api = rustified_core::api::Catalog::new(None).await?;
            let tracks = api
                .search_filtered(
                    "Rick Astley Never Gonna Give You Up",
                    rustified_core::model::SearchFilter::Songs,
                )
                .await?;
            let cover = tracks
                .iter()
                .find(|track| {
                    track.cover.contains(".googleusercontent.com/")
                        || track.cover.contains(".ggpht.com/")
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("No Google-hosted artwork returned for the public test search")
                })?;
            let http = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()?;
            for (kind, url) in [
                ("Album", cover.cover.as_str()),
                ("Video", "https://i.ytimg.com/vi/Fixture0001/hqdefault.jpg"),
            ] {
                let small = fetch(&http, url, false)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Browse artwork failed"))?;
                let large = fetch(&http, url, true)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("HD artwork failed"))?;
                anyhow::ensure!(
                    large.width().max(large.height()) >= 720,
                    "HD artwork is smaller than expected"
                );
                anyhow::ensure!(
                    large.width() > small.width() || large.height() > small.height(),
                    "HD artwork did not retain more detail than browse artwork"
                );
                println!(
                    "{kind} artwork: browse {}x{}, expanded {}x{}",
                    small.width(),
                    small.height(),
                    large.width(),
                    large.height()
                );
            }
            Ok(())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn google_artwork_upgrades_dimensions_and_retains_crop_and_fallback() {
        let url = "https://lh3.googleusercontent.com/opaque=w120-c-h120-k-c0x00ffffff-no-l90-rj";
        let urls = candidates(url, 1536);
        assert_eq!(
            urls[0],
            "https://lh3.googleusercontent.com/opaque=w1536-c-h1536-k-c0x00ffffff-no-l90-rj"
        );
        assert_eq!(urls[1], url);
        assert!(candidates("https://example.com/a=w120-h120", 1536).is_empty());
        assert!(candidates("http://i.ytimg.com/vi/Fixture0001/hqdefault.jpg", 1536).is_empty());
    }
    #[test]
    fn video_artwork_tries_hd_before_standard_and_original() {
        let url = "https://i.ytimg.com/vi/Fixture0001/hqdefault.jpg?sqp=fixture";
        assert_eq!(
            candidates(url, 1536),
            vec![
                "https://i.ytimg.com/vi/Fixture0001/maxresdefault.jpg",
                "https://i.ytimg.com/vi/Fixture0001/sddefault.jpg",
                url
            ]
        );
    }
    #[test]
    fn decoder_keeps_hd_detail_but_never_upscales_a_small_source() {
        for (width, height, edge, expected) in [
            (2048, 1024, 1536, [1536, 768]),
            (2048, 1024, 512, [512, 256]),
            (64, 32, 1536, [64, 32]),
        ] {
            let image = image::RgbaImage::from_pixel(width, height, image::Rgba([30, 60, 90, 255]));
            let mut png = Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png).unwrap();
            assert_eq!(decode(png.get_ref(), edge).unwrap().size, expected);
        }
    }
}
