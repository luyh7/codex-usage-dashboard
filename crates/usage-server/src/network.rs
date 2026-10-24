use std::{io, net::SocketAddr, time::Duration};
use tokio::net::{TcpListener, TcpStream};

// Windows can retain a WSL mirrored binding after its listener has exited.
// Allow TCP cleanup to finish without sharing the port or changing the URL.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn bind(address: SocketAddr) -> io::Result<TcpListener> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    let mut waiting = false;
    loop {
        match TcpListener::bind(address).await {
            Ok(listener) => return Ok(listener),
            Err(error) => {
                let transient = cfg!(windows)
                    && error.kind() == io::ErrorKind::AddrInUse
                    && tokio::time::timeout(
                        Duration::from_millis(200),
                        TcpStream::connect(address),
                    )
                    .await
                    .map_or(true, |result| result.is_err());
                if !transient || tokio::time::Instant::now() >= deadline {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "port {} unavailable: {error}. Another listener, a Windows port reservation, or a previous WSL binding may own it; no process was terminated and no alternate port was used",
                            address.port()
                        ),
                    ));
                }
                if !waiting {
                    eprintln!(
                        "Waiting for Windows to release port {} after the previous listener exited (up to 5 minutes).",
                        address.port()
                    );
                    waiting = true;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}
