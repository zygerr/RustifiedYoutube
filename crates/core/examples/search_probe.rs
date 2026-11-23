use rustified_core::api::Catalog;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api = Catalog::new(None).await?;
    for query in ["rick astley", "rick astley never gonna give you up"] {
        println!("Query: {query}");
        for track in api.search(query).await?.into_iter().take(8) {
            println!(
                "{} | {} | {} | {}",
                track.id, track.title, track.artist, track.duration
            );
        }
    }
    Ok(())
}
