use rustified_core::{resolver, session};
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cookie = session::load_cookie()?;
    let started = Instant::now();
    let _audio = resolver::prepare_with_status("Fixture0001", cookie.as_deref(), |stage| {
        println!("{:.2}s: {stage}", started.elapsed().as_secs_f64());
    })
    .await?;
    println!("Prepared in {:.2}s", started.elapsed().as_secs_f64());
    Ok(())
}
