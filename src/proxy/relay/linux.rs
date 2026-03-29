use std::io;
use std::net::TcpStream;
use std::os::fd::OwnedFd;
use std::thread;

use rustix::io::Errno;
use rustix::pipe::{pipe_with, splice, PipeFlags, SpliceFlags};

use crate::proxy::traffic::ConnectionCounters;

use super::{shutdown_write, spawn_relay_worker, RelayMode, RelayStats};

const PIPE_CHUNK_SIZE: usize = 4 * 1024;
const SPLICE_FLAGS: SpliceFlags = SpliceFlags::MOVE.union(SpliceFlags::NONBLOCK);
const WOULD_BLOCK_RETRY_LIMIT: usize = 32;

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
    counters: ConnectionCounters,
) -> io::Result<RelayStats> {
    let client_read = client.try_clone()?;
    let client_write = client;
    let upstream_read = upstream.try_clone()?;
    let upstream_write = upstream;
    let upload_counters = counters.clone();
    let download_counters = counters;

    let upload = spawn_relay_worker("splice-upload", move || {
        let copied = splice_copy(
            &client_read,
            &upstream_write,
            &pipes.upload.read_end,
            &pipes.upload.write_end,
            &upload_counters,
            true,
        )?;
        shutdown_write(&upstream_write);
        Ok(copied)
    })?;

    let download_bytes = splice_copy(
        &upstream_read,
        &client_write,
        &pipes.download.read_end,
        &pipes.download.write_end,
        &download_counters,
        false,
    )?;
    shutdown_write(&client_write);

    let upload_bytes = upload
        .join()
        .map_err(|_| io::Error::other("upload splice thread panicked"))??;

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
    counters: &ConnectionCounters,
    upload_direction: bool,
) -> io::Result<u64> {
    let mut total = 0_u64;

    loop {
        let moved_into_pipe = match splice_retry(src, pipe_write, PIPE_CHUNK_SIZE)? {
            Some(written) => written,
            None => return relay_with_copy(src, dst, total),
        };

        if moved_into_pipe == 0 {
            return Ok(total);
        }

        let mut remaining = moved_into_pipe;
        while remaining > 0 {
            let moved_from_pipe = match splice_retry(pipe_read, dst, remaining)? {
                Some(written) => written,
                None => return relay_with_copy(src, dst, total),
            };

            if moved_from_pipe == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "splice wrote zero bytes to destination",
                ));
            }

            remaining -= moved_from_pipe;
            let bytes = moved_from_pipe as u64;
            total += bytes;
            if upload_direction {
                counters.add_upload(bytes);
            } else {
                counters.add_download(bytes);
            }
        }
    }
}

fn splice_retry(
    src: impl std::os::fd::AsFd,
    dst: impl std::os::fd::AsFd,
    len: usize,
) -> io::Result<Option<usize>> {
    let mut would_block_retries = 0;

    loop {
        match splice(src.as_fd(), None, dst.as_fd(), None, len, SPLICE_FLAGS) {
            Ok(written) => return Ok(Some(written)),
            Err(error) if error == Errno::INTR => continue,
            Err(error) if error == Errno::AGAIN => {
                would_block_retries += 1;
                if would_block_retries >= WOULD_BLOCK_RETRY_LIMIT {
                    return Ok(None);
                }
                thread::yield_now();
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn relay_with_copy(src: &TcpStream, dst: &TcpStream, already_copied: u64) -> io::Result<u64> {
    tracing::warn!(
        already_copied,
        "splice path is unstable for this flow, falling back to standard copy"
    );

    let mut src = src.try_clone()?;
    let mut dst = dst.try_clone()?;
    let copied = io::copy(&mut src, &mut dst)?;
    Ok(already_copied + copied)
}
