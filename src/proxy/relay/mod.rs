use std::fmt;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

use super::config::RelayMode as ConfigRelayMode;
use super::traffic::ConnectionCounters;

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

    let upload = thread::spawn(move || -> io::Result<u64> {
        let copied = copy_with_counters(
            &mut client_read,
            &mut upstream_write,
            &upload_counters,
            true,
        )?;
        let _ = upstream_write.shutdown(Shutdown::Write);
        Ok(copied)
    });

    let download = thread::spawn(move || -> io::Result<u64> {
        let copied = copy_with_counters(
            &mut upstream_read,
            &mut client_write,
            &download_counters,
            false,
        )?;
        let _ = client_write.shutdown(Shutdown::Write);
        Ok(copied)
    });

    let upload_bytes = upload
        .join()
        .map_err(|_| io::Error::other("upload relay thread panicked"))??;
    let download_bytes = download
        .join()
        .map_err(|_| io::Error::other("download relay thread panicked"))??;

    Ok(RelayStats {
        upload_bytes,
        download_bytes,
        mode: Some(RelayMode::StandardCopy),
    })
}

fn copy_with_counters(
    reader: &mut impl io::Read,
    writer: &mut impl io::Write,
    counters: &ConnectionCounters,
    upload_direction: bool,
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

        if upload_direction {
            counters.add_upload(bytes);
        } else {
            counters.add_download(bytes);
        }
    }
}
