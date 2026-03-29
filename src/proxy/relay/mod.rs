use std::fmt;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

use super::config::RelayMode as ConfigRelayMode;
use super::traffic::ConnectionCounters;

pub(super) const RELAY_THREAD_STACK_SIZE: usize = 128 * 1024;

#[cfg(target_os = "linux")]
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

pub fn relay_bidirectional(
    client: TcpStream,
    upstream: TcpStream,
    counters: ConnectionCounters,
    #[cfg(target_os = "linux")] config_mode: ConfigRelayMode,
    #[cfg(not(target_os = "linux"))] _config_mode: ConfigRelayMode,
) -> io::Result<RelayStats> {
    #[cfg(target_os = "linux")]
    {
        if matches!(config_mode, ConfigRelayMode::LinuxSplice) {
            tracing::warn!(
                "linux splice relay favors throughput and may increase latency jitter for Minecraft gameplay"
            );
            if let Some(pipes) = linux::prepare_pipes() {
                return linux::relay_with_splice(client, upstream, pipes, counters);
            }

            tracing::warn!("falling back to standard relay because splice pipes are unavailable");
        }
    }

    relay_with_copy(client, upstream, counters)
}

#[cfg(target_os = "linux")]
pub(crate) fn shutdown_write(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
}

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

    let upload = spawn_relay_worker("relay-upload", move || {
        let copied =
            copy_upload_with_counters(&mut client_read, &mut upstream_write, &upload_counters)?;
        let _ = upstream_write.shutdown(Shutdown::Write);
        Ok(copied)
    })?;

    let download_bytes =
        copy_download_with_counters(&mut upstream_read, &mut client_write, &download_counters)?;
    let _ = client_write.shutdown(Shutdown::Write);

    let upload_bytes = join_copy_direction(upload, "upload")?;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}

fn join_copy_direction(
    handle: thread::JoinHandle<io::Result<u64>>,
    direction: &str,
) -> io::Result<u64> {
    handle
        .join()
        .map_err(|_| io::Error::other(format!("{direction} relay thread panicked")))?
}

pub(super) fn spawn_relay_worker<F>(
    name: &str,
    work: F,
) -> io::Result<thread::JoinHandle<io::Result<u64>>>
where
    F: FnOnce() -> io::Result<u64> + Send + 'static,
{
    thread::Builder::new()
        .name(name.to_string())
        .stack_size(RELAY_THREAD_STACK_SIZE)
        .spawn(work)
        .map_err(|error| io::Error::other(format!("spawn {name} thread: {error}")))
}

fn copy_upload_with_counters(
    reader: &mut impl io::Read,
    writer: &mut impl io::Write,
    counters: &ConnectionCounters,
) -> io::Result<u64> {
    copy_with_counter(reader, writer, counters, ConnectionCounters::add_upload)
}

fn copy_download_with_counters(
    reader: &mut impl io::Read,
    writer: &mut impl io::Write,
    counters: &ConnectionCounters,
) -> io::Result<u64> {
    copy_with_counter(reader, writer, counters, ConnectionCounters::add_download)
}

fn copy_with_counter(
    reader: &mut impl io::Read,
    writer: &mut impl io::Write,
    counters: &ConnectionCounters,
    add_bytes: fn(&ConnectionCounters, u64),
) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut buf = [0_u8; 16 * 1024];

    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            return Ok(total);
        }

        writer.write_all(&buf[..read])?;
        let bytes = read as u64;
        total += bytes;

        add_bytes(counters, bytes);
    }
}
