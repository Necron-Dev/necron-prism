use std::io;
use std::net::TcpStream;
use std::os::fd::OwnedFd;
use std::thread;

use rustix::fd::AsFd;
use rustix::pipe::{PipeFlags, SpliceFlags, pipe_with, splice};

use super::{RelayMode, RelayStats, shutdown_write};

const PIPE_CHUNK_SIZE: usize = 64 * 1024;
const SPLICE_FLAGS: SpliceFlags = SpliceFlags::MOVE.union(SpliceFlags::MORE);

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
        let (read_end, write_end) = pipe_with(PipeFlags::CLOEXEC)?;
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
            &client_read,
            &upstream_write,
            &pipes.upload.read_end,
            &pipes.upload.write_end,
        )?;
        shutdown_write(&upstream_write);
        Ok(copied)
    });

    let download = thread::spawn(move || -> io::Result<u64> {
        let copied = splice_copy(
            &upstream_read,
            &client_write,
            &pipes.download.read_end,
            &pipes.download.write_end,
        )?;
        shutdown_write(&client_write);
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
    src: &TcpStream,
    dst: &TcpStream,
    pipe_read: &OwnedFd,
    pipe_write: &OwnedFd,
) -> io::Result<u64> {
    let mut total = 0_u64;

    loop {
        let moved_into_pipe = splice(src, None, pipe_write, None, PIPE_CHUNK_SIZE, SPLICE_FLAGS)?;
        if moved_into_pipe == 0 {
            return Ok(total);
        }

        let mut remaining = moved_into_pipe;
        while remaining > 0 {
            let moved_from_pipe = splice(pipe_read, None, dst, None, remaining, SPLICE_FLAGS)?;
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
