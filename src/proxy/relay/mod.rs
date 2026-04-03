use std::fmt;
use std::io;

#[cfg(target_os = "linux")]
use std::net::{Shutdown, TcpStream};

use socket2::SockRef;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::network::apply_sockref_options;

use super::config::{RelayMode as ConfigRelayMode, SocketOptions};
use super::traffic::ConnectionCounters;

mod linux;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayMode {
    StandardCopy,
    #[cfg(target_os = "linux")]
    LinuxSplice,
}

impl RelayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardCopy => "standard-copy",
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
    counters: ConnectionCounters,
    socket_options: &SocketOptions,
    #[allow(unused_variables)]
    config_mode: ConfigRelayMode,
) -> io::Result<RelayStats> {
    let _ = apply_sockref_options(SockRef::from(&client), socket_options);
    let _ = apply_sockref_options(SockRef::from(&upstream), socket_options);

    #[cfg(target_os = "linux")]
    {
        if matches!(config_mode, ConfigRelayMode::LinuxSplice) {
            tracing::warn!(
                "linux splice relay favors throughput and may increase latency jitter for Minecraft gameplay"
            );

            let client = client.into_std()?;
            let upstream = upstream.into_std()?;
            let counters_for_task = counters.clone();

            let stats = tokio::task::spawn_blocking(move || {
                if let Some(pipes) = linux::prepare_pipes() {
                    return linux::relay_with_splice(client, upstream, pipes, counters_for_task);
                }

                tracing::warn!(
                    "falling back to standard relay because splice pipes are unavailable"
                );
                relay_with_copy(client, upstream, counters_for_task)
            })
            .await
            .map_err(|error| io::Error::other(format!("splice relay task panicked: {error}")))??;

            return Ok(stats);
        }
    }

    let (upload_bytes, download_bytes) = relay(client, upstream, counters.clone()).await?;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}

/// Fixed relay buffer size: 32KB stack buffer avoids heap allocation
/// Tradeoff: Uses 64KB stack per relay direction (2 directions = 128KB total)
/// Benefit: Zero heap allocation, better cache locality, stable latency
const RELAY_BUFFER_SIZE: usize = 32 * 1024;

async fn relay(
    client: tokio::net::TcpStream,
    upstream: tokio::net::TcpStream,
    counters: ConnectionCounters,
) -> io::Result<(u64, u64)> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    let upload_counters = counters.clone();
    let download_counters = counters;

    tokio::try_join!(
        custom_async_copy(&mut client_read, &mut upstream_write, upload_counters, true),
        custom_async_copy(&mut upstream_read, &mut client_write, download_counters, false),
    )
}

/// Zero-allocation relay using stack buffer.
/// Stack buffer is pre-allocated, no heap allocation per call.
async fn custom_async_copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    counters: ConnectionCounters,
    upload_direction: bool,
) -> io::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut total = 0_u64;
    // Stack-allocated buffer: no heap allocation, better cache locality
    let mut buf = [0_u8; RELAY_BUFFER_SIZE];

    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            writer.shutdown().await?;
            return Ok(total);
        }

        writer.write_all(&buf[..read]).await?;
        let bytes = read as u64;
        total += bytes;
        if upload_direction {
            counters.add_upload(bytes);
        } else {
            counters.add_download(bytes);
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
    counters: ConnectionCounters,
) -> io::Result<RelayStats> {
    let mut client_read = client.try_clone()?;
    let mut client_write = client;
    let mut upstream_read = upstream.try_clone()?;
    let mut upstream_write = upstream;

    let upload_counters = counters.clone();
    let download_counters = counters;

    let upload = std::thread::Builder::new()
        .name("relay-upload".to_string())
        .spawn(move || {
            let copied = io::copy(&mut client_read, &mut upstream_write)?;
            upload_counters.add_upload(copied);
            let _ = upstream_write.shutdown(Shutdown::Write);
            Ok::<u64, io::Error>(copied)
        })
        .map_err(|error| io::Error::other(format!("spawn relay-upload thread: {error}")))?;

    let download_bytes = io::copy(&mut upstream_read, &mut client_write)?;
    download_counters.add_download(download_bytes);
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
