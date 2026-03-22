use std::fmt;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

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

pub fn relay_bidirectional(client: TcpStream, upstream: TcpStream) -> io::Result<RelayStats> {
    #[cfg(target_os = "linux")]
    {
        if let Some(pipes) = linux::prepare_pipes() {
            return linux::relay_with_splice(client, upstream, pipes);
        }

        tracing::warn!("falling back to standard relay because splice pipes are unavailable");
    }

    relay_with_copy(client, upstream)
}

#[cfg(target_os = "linux")]
pub(crate) fn shutdown_write(stream: &TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
}

fn relay_with_copy(client: TcpStream, upstream: TcpStream) -> io::Result<RelayStats> {
    let mut client_read = client.try_clone()?;
    let mut client_write = client;
    let mut upstream_read = upstream.try_clone()?;
    let mut upstream_write = upstream;

    let upload = thread::spawn(move || -> io::Result<u64> {
        let copied = io::copy(&mut client_read, &mut upstream_write)?;
        let _ = upstream_write.shutdown(Shutdown::Write);
        Ok(copied)
    });

    let download = thread::spawn(move || -> io::Result<u64> {
        let copied = io::copy(&mut upstream_read, &mut client_write)?;
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
