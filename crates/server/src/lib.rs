pub mod db;
pub mod files;
pub mod gateway;
pub mod hub;
pub mod media;
pub mod tls;

use std::{path::PathBuf, sync::Arc};

pub struct Config {
    pub quic_addr: std::net::SocketAddr,
    pub udp_addr: std::net::SocketAddr,
    /// Address clients should send media to (public ip:port).
    pub udp_advertise: String,
    pub db_url: String,
    pub data_dir: PathBuf,
}

pub struct AppState {
    pub pool: deadpool_postgres::Pool,
    pub hub: hub::Hub,
    pub media: media::MediaState,
    pub files: files::BlobStore,
    pub udp_advertise: String,
}

pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let pool = db::make_pool(&cfg.db_url)?;
    db::migrate(&pool).await?;
    std::fs::create_dir_all(&cfg.data_dir)?;
    let state = Arc::new(AppState {
        pool,
        hub: hub::Hub::default(),
        media: media::MediaState::default(),
        files: files::BlobStore::new(cfg.data_dir.join("blobs"))?,
        udp_advertise: cfg.udp_advertise.clone(),
    });
    let sock = Arc::new(tokio::net::UdpSocket::bind(cfg.udp_addr).await?);
    tracing::info!("media UDP on {}", sock.local_addr()?);
    let media_task = tokio::spawn(media::run_udp(state.clone(), sock));
    let gw_task = tokio::spawn(gateway::run(state.clone(), cfg.quic_addr, cfg.data_dir.clone()));
    tokio::select! {
        r = media_task => r??,
        r = gw_task => r??,
    }
    Ok(())
}
