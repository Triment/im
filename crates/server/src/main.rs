use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};
use xmp_server::Config;

#[derive(Parser)]
#[command(name = "xmp-server", version, about = "XMP IM / voice / video server")]
struct Args {
    /// QUIC control-plane listen address
    #[arg(long, default_value = "0.0.0.0:4433")]
    quic: SocketAddr,
    /// UDP media-plane listen address
    #[arg(long, default_value = "0.0.0.0:4434")]
    udp: SocketAddr,
    /// Public ip:port advertised to clients for media (defaults to --udp)
    #[arg(long)]
    udp_public: Option<String>,
    /// PostgreSQL connection string
    #[arg(long, env = "XMP_DB", default_value = "host=localhost user=postgres dbname=xmp")]
    db: String,
    #[arg(long, default_value = "./data")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let a = Args::parse();
    xmp_server::run(Config {
        quic_addr: a.quic,
        udp_addr: a.udp,
        udp_advertise: a.udp_public.unwrap_or_else(|| a.udp.to_string()),
        db_url: a.db,
        data_dir: a.data_dir,
    })
    .await
}
