use std::fmt;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

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

#[cfg(target_os = "linux")]
mod linux {
    use super::{RelayMode, RelayStats};
    use std::io;
    use std::net::{Shutdown, TcpStream};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
    use std::thread;

    const PIPE_CHUNK_SIZE: usize = 64 * 1024;
    const SPLICE_FLAGS: u32 = libc::SPLICE_F_MOVE | libc::SPLICE_F_MORE;

    pub struct SplicePipes {
        upload: SplicePipe,
        download: SplicePipe,
    }

    struct SplicePipe {
        read_end: OwnedFd,
        write_end: OwnedFd,
    }

    impl SplicePipe {
        fn new() -> io::Result<Self> {
            let mut fds = [0; 2];
            let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }

            let read_end = unsafe { OwnedFd::from_raw_fd(fds[0]) };
            let write_end = unsafe { OwnedFd::from_raw_fd(fds[1]) };
            Ok(Self {
                read_end,
                write_end,
            })
        }
    }

    pub fn prepare_pipes() -> Option<SplicePipes> {
        let upload = match SplicePipe::new() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::warn!(error = %error, "failed to create upload splice pipe");
                return None;
            }
        };

        let download = match SplicePipe::new() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::warn!(error = %error, "failed to create download splice pipe");
                return None;
            }
        };

        Some(SplicePipes { upload, download })
    }

    pub fn relay_with_splice(
        client: TcpStream,
        upstream: TcpStream,
        pipes: SplicePipes,
    ) -> io::Result<RelayStats> {
        let client_read = client.try_clone()?;
        let client_write = client;
        let upstream_read = upstream.try_clone()?;
        let upstream_write = upstream;

        let upload = thread::spawn(move || -> io::Result<u64> {
            let copied = splice_copy(
                client_read.as_raw_fd(),
                upstream_write.as_raw_fd(),
                pipes.upload.read_end.as_raw_fd(),
                pipes.upload.write_end.as_raw_fd(),
            )?;
            let _ = upstream_write.shutdown(Shutdown::Write);
            Ok(copied)
        });

        let download = thread::spawn(move || -> io::Result<u64> {
            let copied = splice_copy(
                upstream_read.as_raw_fd(),
                client_write.as_raw_fd(),
                pipes.download.read_end.as_raw_fd(),
                pipes.download.write_end.as_raw_fd(),
            )?;
            let _ = client_write.shutdown(Shutdown::Write);
            Ok(copied)
        });

        let upload_bytes = upload
            .join()
            .map_err(|_| io::Error::other("upload splice thread panicked"))??;
        let download_bytes = download
            .join()
            .map_err(|_| io::Error::other("download splice thread panicked"))??;

        Ok(RelayStats {
            upload_bytes,
            download_bytes,
            mode: Some(RelayMode::LinuxSplice),
        })
    }

    fn splice_copy(
        src_fd: RawFd,
        dst_fd: RawFd,
        pipe_read_fd: RawFd,
        pipe_write_fd: RawFd,
    ) -> io::Result<u64> {
        let mut total = 0_u64;

        loop {
            let moved_into_pipe = splice_once(src_fd, pipe_write_fd, PIPE_CHUNK_SIZE)?;
            if moved_into_pipe == 0 {
                return Ok(total);
            }

            let mut remaining = moved_into_pipe;
            while remaining > 0 {
                let moved_from_pipe = splice_once(pipe_read_fd, dst_fd, remaining)?;
                if moved_from_pipe == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "splice wrote zero bytes to destination",
                    ));
                }

                remaining -= moved_from_pipe;
                total += moved_from_pipe as u64;
            }
        }
    }

    fn splice_once(src_fd: RawFd, dst_fd: RawFd, len: usize) -> io::Result<usize> {
        loop {
            let result = unsafe {
                libc::splice(
                    src_fd,
                    std::ptr::null_mut(),
                    dst_fd,
                    std::ptr::null_mut(),
                    len,
                    SPLICE_FLAGS,
                )
            };
            if result >= 0 {
                return Ok(result as usize);
            }

            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }

            return Err(error);
        }
    }
}
