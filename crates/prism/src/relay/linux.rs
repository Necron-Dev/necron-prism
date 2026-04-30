#![cfg_attr(
    not(all(target_os = "linux", feature = "linux-accel")),
    allow(dead_code, unused_imports)
)]

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
mod imp {
    use std::fmt;
    use std::io;
    use std::net::TcpStream;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::sync::{Arc, OnceLock};
    use std::thread;

    use crossbeam_channel::{self, Sender};
    use rustix::io::Errno;
    use rustix::pipe::{PipeFlags, SpliceFlags, pipe_with, splice};
    use tokio::sync::mpsc::{self, error::TrySendError as TokioTrySendError};
    use tokio_uring::buf::BoundedBuf;

    use crate::session::ConnectionSession;

    use crate::relay::{RelayMode, RelayStats, shutdown_write, spawn_relay_worker};

    const SPLICE_FLAGS: SpliceFlags = SpliceFlags::MOVE.union(SpliceFlags::NONBLOCK);
    const WOULD_BLOCK_RETRY_LIMIT: usize = 8;
    const IO_URING_WORKER_QUEUE_CAPACITY: usize = 1024;
    const POLL_TIMEOUT_MS: i32 = 50;

    pub struct SplicePipes {
        upload: SplicePipe,
        download: SplicePipe,
    }

    struct SplicePipe {
        read_end: OwnedFd,
        write_end: OwnedFd,
    }

    pub enum IoUringRelayError {
        Unavailable(io::Error),
        Relay(io::Error),
    }

    struct IoUringRelayJob {
        client: TcpStream,
        upstream: TcpStream,
        session: ConnectionSession,
        response_tx: Sender<io::Result<RelayStats>>,
        buffer_size: usize,
    }

    struct SharedIoUringWorker {
        submitter: mpsc::Sender<IoUringRelayJob>,
    }

    impl fmt::Display for IoUringRelayError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Unavailable(error) | Self::Relay(error) => error.fmt(f),
            }
        }
    }

    impl IoUringRelayError {
        pub fn is_unavailable(&self) -> bool {
            matches!(self, Self::Unavailable(_))
        }

        pub fn into_io(self) -> io::Error {
            match self {
                Self::Unavailable(error) | Self::Relay(error) => error,
            }
        }
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

    impl SharedIoUringWorker {
        fn shared() -> io::Result<&'static Self> {
            static WORKER: OnceLock<io::Result<SharedIoUringWorker>> = OnceLock::new();

            match WORKER.get_or_init(Self::start) {
                Ok(worker) => Ok(worker),
                Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
            }
        }

        fn start() -> io::Result<Self> {
            let (submitter, mut receiver) =
                mpsc::channel::<IoUringRelayJob>(IO_URING_WORKER_QUEUE_CAPACITY);
            let (ready_tx, ready_rx) = crossbeam_channel::bounded::<io::Result<()>>(1);

            thread::Builder::new()
                .name("relay-io-uring".to_string())
                .spawn(move || {
                    tokio_uring::start(async move {
                        let _ = ready_tx.send(Ok(()));

                        while let Some(job) = receiver.recv().await {
                            tokio_uring::spawn(async move {
                                let result = Self::run_io_uring_relay(
                                    job.client,
                                    job.upstream,
                                    job.session,
                                    job.buffer_size,
                                )
                                .await;
                                let _ = job.response_tx.send(result);
                            });
                        }
                    });
                })
                .map_err(|error| {
                    io::Error::other(format!("spawn relay-io-uring thread: {error}"))
                })?;

            ready_rx
                .recv()
                .map_err(|_| io::Error::other("shared io_uring worker failed to initialize"))??;

            Ok(Self { submitter })
        }

        fn relay(
            &self,
            client: TcpStream,
            upstream: TcpStream,
            session: ConnectionSession,
            buffer_size: usize,
        ) -> Result<RelayStats, IoUringRelayError> {
            let (response_tx, response_rx) = crossbeam_channel::bounded(1);
            let job = IoUringRelayJob {
                client,
                upstream,
                session,
                response_tx,
                buffer_size,
            };

            self.submitter.try_send(job).map_err(|error| match error {
                TokioTrySendError::Full(_) => IoUringRelayError::Unavailable(io::Error::other(
                    "shared io_uring worker queue is full",
                )),
                TokioTrySendError::Closed(_) => IoUringRelayError::Unavailable(io::Error::other(
                    "shared io_uring worker is unavailable",
                )),
            })?;

            match response_rx.recv() {
                Ok(Ok(stats)) => Ok(stats),
                Ok(Err(error)) => Err(IoUringRelayError::Relay(error)),
                Err(_) => Err(IoUringRelayError::Unavailable(io::Error::other(
                    "shared io_uring worker dropped relay response",
                ))),
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
            session: ConnectionSession,
            pipe_chunk_size: usize,
        ) -> io::Result<RelayStats> {
            tracing::trace!("[CONNECT/RELAY] starting linux-splice relay");

            let client_read = client.try_clone()?;
            let client_write = client;
            let upstream_read = upstream.try_clone()?;
            let upstream_write = upstream;
            let upload_session = session.clone();
            let download_session = session;

            let upload = spawn_relay_worker("splice-upload", move || {
                let copied = Self::splice_copy(
                    &client_read,
                    &upstream_write,
                    &pipes.upload.read_end,
                    &pipes.upload.write_end,
                    &upload_session,
                    true,
                    pipe_chunk_size,
                )?;
                shutdown_write(&upstream_write);
                tracing::trace!(
                    direction = "upload",
                    bytes = copied,
                    "[CONNECT/RELAY] direction finished"
                );
                Ok(copied)
            })?;

            let download_bytes = Self::splice_copy(
                &upstream_read,
                &client_write,
                &pipes.download.read_end,
                &pipes.download.write_end,
                &download_session,
                false,
                pipe_chunk_size,
            )?;
            shutdown_write(&client_write);
            tracing::trace!(
                direction = "download",
                bytes = download_bytes,
                "[CONNECT/RELAY] direction finished"
            );

            let upload_bytes = upload
                .join()
                .map_err(|_| io::Error::other("upload splice thread panicked"))??;

            Ok(RelayStats {
                upload_bytes,
                download_bytes,
                mode: Some(RelayMode::LinuxSplice),
            })
        }

        #[allow(dead_code)]
        pub fn relay_with_io_uring(
            client: TcpStream,
            upstream: TcpStream,
            session: ConnectionSession,
            buffer_size: usize,
        ) -> Result<RelayStats, IoUringRelayError> {
            SharedIoUringWorker::shared()
                .map_err(IoUringRelayError::Unavailable)?
                .relay(client, upstream, session, buffer_size)
        }

        fn splice_copy(
            src: &TcpStream,
            dst: &TcpStream,
            pipe_read: &OwnedFd,
            pipe_write: &OwnedFd,
            session: &ConnectionSession,
            upload_direction: bool,
            pipe_chunk_size: usize,
        ) -> io::Result<u64> {
            let mut total = 0_u64;

            loop {
                let moved_into_pipe = match Self::splice_retry(src, pipe_write, pipe_chunk_size)? {
                    Some(written) => written,
                    None => {
                        return Self::relay_with_copy_fallback(
                            src,
                            dst,
                            total,
                            session,
                            upload_direction,
                        );
                    }
                };

                if moved_into_pipe == 0 {
                    return Ok(total);
                }

                let mut remaining = moved_into_pipe;
                while remaining > 0 {
                    let moved_from_pipe = match Self::splice_retry(pipe_read, dst, remaining)? {
                        Some(written) => written,
                        None => {
                            return Self::relay_with_copy_fallback(
                                src,
                                dst,
                                total,
                                session,
                                upload_direction,
                            );
                        }
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
                        session.add_upload(bytes);
                    } else {
                        session.add_download(bytes);
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
                        // Use poll(2) to wait for fd readiness instead of busy-wait yield_now().
                        // This avoids sched_yield() jitter spikes (100μs-10ms) under CPU contention.
                        let src_fd = src.as_fd().as_raw_fd();
                        let mut pfd = libc::pollfd {
                            fd: src_fd,
                            events: libc::POLLIN | libc::POLLOUT,
                            revents: 0,
                        };
                        // Safety: single pollfd, valid fd, bounded timeout
                        unsafe { libc::poll(&mut pfd, 1, POLL_TIMEOUT_MS) };
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }

        fn relay_with_copy_fallback(
            src: &TcpStream,
            dst: &TcpStream,
            already_copied: u64,
            session: &ConnectionSession,
            upload_direction: bool,
        ) -> io::Result<u64> {
            tracing::warn!(
                already_copied,
                "splice path is unstable for this flow, falling back to standard copy"
            );

            let mut src = src.try_clone()?;
            let mut dst = dst.try_clone()?;
            let copied = io::copy(&mut src, &mut dst)?;
            if upload_direction {
                session.add_upload(copied);
            } else {
                session.add_download(copied);
            }
            Ok(already_copied + copied)
        }

        async fn io_uring_copy(
            reader: Arc<tokio_uring::net::TcpStream>,
            writer: Arc<tokio_uring::net::TcpStream>,
            session: ConnectionSession,
            upload_direction: bool,
            buffer_size: usize,
        ) -> io::Result<u64> {
            let direction = if upload_direction {
                "upload"
            } else {
                "download"
            };
            let mut total = 0_u64;
            let mut buf = vec![0_u8; buffer_size];

            loop {
                let (read_result, next_buf) = reader.read(buf).await;
                buf = next_buf;
                match read_result {
                    Ok(0) => {
                        let _ = writer.shutdown(std::net::Shutdown::Write);
                        tracing::trace!(
                            direction,
                            bytes = total,
                            "[CONNECT/RELAY] direction finished (EOF)"
                        );
                        return Ok(total);
                    }
                    Ok(read) => {
                        let (write_result, next_buf) = writer.write_all(buf.slice(..read)).await;
                        if let Err(error) = write_result {
                            tracing::trace!(direction, bytes = total, error = %error, "[CONNECT/RELAY] direction write failed");
                            return Err(error);
                        }
                        buf = next_buf.into_inner();

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

        #[allow(clippy::arc_with_non_send_sync)]
        async fn run_io_uring_relay(
            client: TcpStream,
            upstream: TcpStream,
            session: ConnectionSession,
            buffer_size: usize,
        ) -> io::Result<RelayStats> {
            tracing::trace!("[CONNECT/RELAY] starting io-uring relay");

            let client = tokio_uring::net::TcpStream::from_std(client);
            let upstream = tokio_uring::net::TcpStream::from_std(upstream);
            let client = Arc::new(client);
            let upstream = Arc::new(upstream);
            let upload_session = session.clone();
            let download_session = session;

            let upload_client = Arc::clone(&client);
            let upload_upstream = Arc::clone(&upstream);
            let upload = tokio_uring::spawn(async move {
                Self::io_uring_copy(
                    upload_client,
                    upload_upstream,
                    upload_session,
                    true,
                    buffer_size,
                )
                .await
            });

            let download_bytes =
                Self::io_uring_copy(upstream, client, download_session, false, buffer_size).await?;
            let upload_bytes = upload.await.map_err(|error| {
                io::Error::other(format!("io_uring upload task failed: {error}"))
            })??;

            Ok(RelayStats {
                upload_bytes,
                download_bytes,
                mode: Some(RelayMode::IoUring),
            })
        }
    }

    pub fn relay_with_io_uring(
        client: TcpStream,
        upstream: TcpStream,
        session: ConnectionSession,
        buffer_size: usize,
    ) -> Result<RelayStats, IoUringRelayError> {
        SharedIoUringWorker::shared()
            .map_err(IoUringRelayError::Unavailable)?
            .relay(client, upstream, session, buffer_size)
    }

    pub fn prepare_pipes() -> Option<SplicePipes> {
        SharedIoUringWorker::prepare_pipes()
    }

    pub fn relay_with_splice(
        client: TcpStream,
        upstream: TcpStream,
        pipes: SplicePipes,
        session: ConnectionSession,
        pipe_chunk_size: usize,
    ) -> io::Result<RelayStats> {
        SharedIoUringWorker::relay_with_splice(client, upstream, pipes, session, pipe_chunk_size)
    }
}
#[cfg(all(target_os = "linux", feature = "linux-accel"))]
pub(super) use imp::*;
