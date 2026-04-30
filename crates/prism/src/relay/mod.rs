use std::fmt;
use std::io;

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
use std::net::{Shutdown, TcpStream};

use socket2::SockRef;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::config::Config;
use crate::network::apply_sockref_options;
use crate::session::ConnectionSession;

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
mod linux;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayMode {
    StandardCopy,
    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    IoUring,
    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    LinuxSplice,
}

impl RelayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardCopy => "standard-copy",
            #[cfg(all(target_os = "linux", feature = "linux-accel"))]
            Self::IoUring => "io-uring",
            #[cfg(all(target_os = "linux", feature = "linux-accel"))]
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
#[allow(dead_code)]
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

    tracing::trace!(
        relay_mode = config.network.relay.label(),
        "[CONNECT/RELAY] starting bidirectional relay"
    );

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    {
        if config.network.relay.is_io_uring() {
            let client = client.into_std()?;
            let upstream = upstream.into_std()?;
            let session_for_task = session.clone();
            let relay_span = session.root_span().clone();
            let buffer_size = config.network.buffer.io_uring_buffer_size;

            let stats = tokio::task::spawn_blocking(move || -> io::Result<RelayStats> {
                let _guard = relay_span.enter();
                match linux::relay_with_io_uring(
                    client.try_clone()?,
                    upstream.try_clone()?,
                    session_for_task.clone(),
                    buffer_size,
                ) {
                    Ok(stats) => Ok(stats),
                    Err(error) if error.is_unavailable() => {
                        tracing::warn!(error = %error, "[CONNECT/RELAY] io_uring relay unavailable, falling back to async relay");
                        relay_with_copy(client, upstream, session_for_task)
                    }
                    Err(error) => Err(error.into_io()),
                }
            })
            .await
            .map_err(|error| io::Error::other(format!("io_uring relay task panicked: {error}")))?;

            return stats;
        }

        if config.network.relay.is_splice() {
            let client = client.into_std()?;
            let upstream = upstream.into_std()?;
            let session_for_task = session.clone();
            let relay_span = session.root_span().clone();
            let pipe_chunk_size = config.network.buffer.splice_pipe_chunk_size;

            let stats = tokio::task::spawn_blocking(move || -> io::Result<RelayStats> {
                let _guard = relay_span.enter();
                if let Some(pipes) = linux::prepare_pipes() {
                    return linux::relay_with_splice(
                        client,
                        upstream,
                        pipes,
                        session_for_task,
                        pipe_chunk_size,
                    );
                }

                tracing::warn!(
                    "[CONNECT/RELAY] splice pipes unavailable, falling back to async relay"
                );
                relay_with_copy(client, upstream, session_for_task)
            })
            .await
            .map_err(|error| io::Error::other(format!("splice relay task panicked: {error}")))?;

            return stats;
        }
    }

    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    {
        let _ = config;
    }

    let buffer_size = config.network.buffer.relay_buffer_size;
    let (upload_bytes, download_bytes) = relay(client, upstream, session, buffer_size).await?;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}

#[cfg(test)]
mod test;

async fn relay(
    mut client: tokio::net::TcpStream,
    mut upstream: tokio::net::TcpStream,
    session: ConnectionSession,
    buffer_size: usize,
) -> io::Result<(u64, u64)> {
    let (mut client_read, mut client_write) = client.split();
    let (mut upstream_read, mut upstream_write) = upstream.split();
    let upload_session = session.clone();
    let download_session = session;

    tokio::try_join!(
        custom_async_copy(
            &mut client_read,
            &mut upstream_write,
            upload_session,
            true,
            buffer_size
        ),
        custom_async_copy(
            &mut upstream_read,
            &mut client_write,
            download_session,
            false,
            buffer_size
        ),
    )
}

async fn custom_async_copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    session: ConnectionSession,
    upload_direction: bool,
    buffer_size: usize,
) -> io::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _guard = session.enter_stage("CONNECT/RELAY");
    let direction = if upload_direction {
        "upload"
    } else {
        "download"
    };
    let mut total = 0_u64;
    let mut buf = vec![0_u8; buffer_size];

    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                let _ = writer.shutdown().await;
                tracing::trace!(
                    direction,
                    bytes = total,
                    "[CONNECT/RELAY] direction finished (EOF)"
                );
                return Ok(total);
            }
            Ok(read) => {
                if let Err(error) = writer.write_all(&buf[..read]).await {
                    tracing::trace!(direction, bytes = total, error = %error, "[CONNECT/RELAY] direction write failed");
                    return Err(error);
                }
                let bytes = read as u64;
                total += bytes;
                if upload_direction {
                    session.add_upload(bytes);
                } else {
                    session.add_download(bytes);
                }
            }
            Err(error) => {
                tracing::trace!(direction, bytes = total, error = %error, "[CONNECT/RELAY] direction read failed");
                return Err(error);
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
pub(crate) fn shutdown_write(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
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

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
fn relay_with_copy(
    client: TcpStream,
    upstream: TcpStream,
    session: ConnectionSession,
) -> io::Result<RelayStats> {
    tracing::trace!("[CONNECT/RELAY] falling back to standard-copy relay");

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
            tracing::trace!(
                direction = "upload",
                bytes = copied,
                "[CONNECT/RELAY] direction finished"
            );
            Ok::<u64, io::Error>(copied)
        })
        .map_err(|error| io::Error::other(format!("spawn relay-upload thread: {error}")))?;

    let download_bytes = io::copy(&mut upstream_read, &mut client_write)?;
    download_session.add_download(download_bytes);
    let _ = client_write.shutdown(Shutdown::Write);
    tracing::trace!(
        direction = "download",
        bytes = download_bytes,
        "[CONNECT/RELAY] direction finished"
    );

    let upload_bytes = upload
        .join()
        .map_err(|_| io::Error::other("relay upload thread panicked"))??;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}
