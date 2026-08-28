use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use fabdev_share::ShareServer;

#[derive(Debug, Parser)]
#[command(
  name = "fabdev-share",
  version,
  about = "Share a local fabDev Site over LAN"
)]
struct Arguments {
  #[arg(long, default_value = "0.0.0.0:18080")]
  listen: SocketAddr,
  #[arg(long, default_value = "127.0.0.1:8080")]
  upstream: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
  let arguments = Arguments::parse();
  let mut server = ShareServer::start(arguments.listen, arguments.upstream).await?;
  println!(
    "fabDev Share listening at {} and forwarding to {}",
    server.local_addr(),
    arguments.upstream
  );
  tokio::signal::ctrl_c().await?;
  server.stop().await
}
