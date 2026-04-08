use std::fmt;
use std::io;

#[cfg(target_os = "linux")]
use std::net::{Shutdown, TcpStream};

use socket2::SockRef;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(not(target_os = "linux"))]
use tracing::warn;

use crate::proxy::config::{Config, RelayDataMode};
use crate::proxy::network::apply_sockref_options;
use crate::proxy::stats::ConnectionSession;

mod linux;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayMode {
    StandardCopy,
    #[cfg(target_os = "linux")]
    IoUring,
    #[cfg(target_os = "linux")]
    LinuxSplice,
}

impl RelayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardCopy => "standard-copy",
            #[cfg(target_os = "linux")]
            Self::IoUring => "io-uring",
            #[cfg(target_os = "linux")]
            Self::LinuxSplice => "linux-splice",
        }
    }
}

impl fmt::Display for RelayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RelayStats {
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub mode: Option<RelayMode>,
}

pub async fn relay_bidirectional(
    client: tokio::net::TcpStream,
    upstream: tokio::net::TcpStream,
    session: ConnectionSession,
    config: &Config,
) -> io::Result<RelayStats> {
    let logging_session = session.clone();
    let _guard = logging_session.enter_stage("CONNECT/RELAY");
    let _ = apply_sockref_options(SockRef::from(&client), config);
    let _ = apply_sockref_options(SockRef::from(&upstream), config);

    tracing::debug!(relay_mode = config.network.relay.label(), "[CONNECT/RELAY] starting bidirectional relay");

    #[cfg(target_os = "linux")]
    {
        match (config.network.relay.mode, config.network.relay.io_uring) {
            (RelayDataMode::Async, true) => {
                let client = client.into_std()?;
                let upstream = upstream.into_std()?;
                let session_for_task = session.clone();
                let relay_span = session.root_span().clone();

                let stats = tokio::task::spawn_blocking(move || -> io::Result<RelayStats> {
                    let _guard = relay_span.enter();
                    match linux::relay_with_io_uring(
                        client.try_clone()?,
                        upstream.try_clone()?,
                        session_for_task.clone(),
                    ) {
                        Ok(stats) => Ok(stats),
                        Err(error) if error.is_unavailable() => {
                            tracing::warn!(error = %error, "[CONNECT/RELAY] io_uring relay unavailable, falling back to standard relay");
                            relay_with_copy(client, upstream, session_for_task)
                        }
                        Err(error) => Err(error.into_io()),
                    }
                })
                .await
                .map_err(|error| io::Error::other(format!("io_uring relay task panicked: {error}")))??;

                return Ok(stats);
            }
            (RelayDataMode::Splice, true) => {
                tracing::warn!("[CONNECT/RELAY] splice mode with io_uring enabled prefers io_uring first, then falls back to splice");

                let client = client.into_std()?;
                let upstream = upstream.into_std()?;
                let session_for_task = session.clone();
                let relay_span = session.root_span().clone();

                let stats = tokio::task::spawn_blocking(move || -> io::Result<RelayStats> {
                    let _guard = relay_span.enter();
                    match linux::relay_with_io_uring(client.try_clone()?, upstream.try_clone()?, session_for_task.clone()) {
                        Ok(stats) => Ok(stats),
                        Err(error) if error.is_unavailable() => {
                            tracing::warn!(error = %error, "[CONNECT/RELAY] io_uring relay unavailable, falling back to splice relay");
                            if let Some(pipes) = linux::prepare_pipes() {
                                return linux::relay_with_splice(client, upstream, pipes, session_for_task);
                            }

                            tracing::warn!("[CONNECT/RELAY] falling back to standard relay because splice pipes are unavailable");
                            relay_with_copy(client, upstream, session_for_task)
                        }
                        Err(error) => Err(error.into_io()),
                    }
                })
                .await
                .map_err(|error| io::Error::other(format!("hybrid relay task panicked: {error}")))??;

                return Ok(stats);
            }
            (RelayDataMode::Splice, false) => {
                tracing::warn!("[CONNECT/RELAY] linux splice relay favors throughput and may increase latency jitter for Minecraft gameplay");

                let client = client.into_std()?;
                let upstream = upstream.into_std()?;
                let session_for_task = session.clone();
                let relay_span = session.root_span().clone();

                let stats = tokio::task::spawn_blocking(move || -> io::Result<RelayStats> {
                    let _guard = relay_span.enter();
                    if let Some(pipes) = linux::prepare_pipes() {
                        return linux::relay_with_splice(client, upstream, pipes, session_for_task);
                    }

                    tracing::warn!("[CONNECT/RELAY] falling back to standard relay because splice pipes are unavailable");
                    relay_with_copy(client, upstream, session_for_task)
                })
                .await
                .map_err(|error| io::Error::other(format!("splice relay task panicked: {error}")))??;

                return Ok(stats);
            }
            (RelayDataMode::Async, false) => {}
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if config.network.relay.io_uring || matches!(config.network.relay.mode, RelayDataMode::Splice) {
            warn!(relay_mode = config.network.relay.label(), "[CONNECT/RELAY] requested Linux-specific relay acceleration is unavailable on this platform, using async relay");
        }
    }

    let (upload_bytes, download_bytes) = relay(client, upstream, session).await?;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}

const RELAY_BUFFER_SIZE: usize = 32 * 1024;

async fn relay(
    client: tokio::net::TcpStream,
    upstream: tokio::net::TcpStream,
    session: ConnectionSession,
) -> io::Result<(u64, u64)> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    let upload_session = session.clone();
    let download_session = session;

    tokio::try_join!(
        custom_async_copy(&mut client_read, &mut upstream_write, upload_session, true),
        custom_async_copy(&mut upstream_read, &mut client_write, download_session, false),
    )
}

async fn custom_async_copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    session: ConnectionSession,
    upload_direction: bool,
) -> io::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _guard = session.enter_stage("CONNECT/RELAY");
    let mut total = 0_u64;
    let mut buf = [0_u8; RELAY_BUFFER_SIZE];

    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            writer.shutdown().await?;
            return Ok(total);
        }
        tracing::trace!(
            direction = if upload_direction { "upload" } else { "download" },
            bytes = read,
            "[CONNECT/RELAY] relay chunk transferred"
        );

        writer.write_all(&buf[..read]).await?;

        let bytes = read as u64;
        total += bytes;
        if upload_direction {
            session.add_upload(bytes);
        } else {
            session.add_download(bytes);
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn shutdown_write(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
}

#[cfg(target_os = "linux")]
pub(super) fn spawn_relay_worker<F>(
    name: &str,
    work: F,
) -> io::Result<std::thread::JoinHandle<io::Result<u64>>>
where
    F: FnOnce() -> io::Result<u64> + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .map_err(|error| io::Error::other(format!("spawn {name} thread: {error}")))
}

#[cfg(target_os = "linux")]
fn relay_with_copy(
    client: TcpStream,
    upstream: TcpStream,
    session: ConnectionSession,
) -> io::Result<RelayStats> {
    let mut client_read = client.try_clone()?;
    let mut client_write = client;
    let mut upstream_read = upstream.try_clone()?;
    let mut upstream_write = upstream;

    let upload_session = session.clone();
    let download_session = session;

    let upload = std::thread::Builder::new()
        .name("relay-upload".to_string())
        .spawn(move || {
            let copied = io::copy(&mut client_read, &mut upstream_write)?;
            upload_session.add_upload(copied);
            let _ = upstream_write.shutdown(Shutdown::Write);
            Ok::<u64, io::Error>(copied)
        })
        .map_err(|error| io::Error::other(format!("spawn relay-upload thread: {error}")))?;

    let download_bytes = io::copy(&mut upstream_read, &mut client_write)?;
    download_session.add_download(download_bytes);
    let _ = client_write.shutdown(Shutdown::Write);

    let upload_bytes = upload
        .join()
        .map_err(|_| io::Error::other("relay upload thread panicked"))??;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}
