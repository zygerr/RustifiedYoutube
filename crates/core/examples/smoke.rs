use rustified_core::api::Catalog;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let authenticated = std::env::args().any(|arg| arg == "--authenticated");
    let cookie = if authenticated {
        rustified_core::session::load_cookie()?
    } else {
        None
    };
    let api = Catalog::new(cookie.as_deref()).await?;
    if authenticated {
        anyhow::ensure!(api.signed_in(), "No native account session");
        let playlists = api.library().await?;
        println!(
            "Authenticated library: {} playlists (names withheld)",
            playlists.len()
        );
        if let Some(playlist) = playlists.iter().find(|p| p.id != "LM") {
            let tracks = api.playlist(&playlist.id).await?;
            println!(
                "Authenticated playlist: {} tracks (names withheld)",
                tracks.len()
            );
        }
    }
    let songs = api.search("Rick Astley Never Gonna Give You Up").await?;
    anyhow::ensure!(!songs.is_empty(), "Search returned no tracks");
    println!("Search: {} tracks", songs.len());
    Ok(())
}
