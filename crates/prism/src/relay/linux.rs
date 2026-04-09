#![cfg_attr(not(all(target_os = "linux", feature = "linux-accel")), allow(dead_code, unused_imports))]

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
mod imp {
    use std::fmt;
    use std::io;
    use std::net::TcpStream;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::sync::{mpsc as std_mpsc, Arc, OnceLock};
    use std::thread;

    use rustix::io::Errno;
    use rustix::pipe::{pipe_with, splice, PipeFlags, SpliceFlags};
    use tokio::sync::mpsc::{self, error::TrySendError};
    use tokio_uring::buf::BoundedBuf;

    use crate::session::ConnectionSession;

    use super::{shutdown_write, spawn_relay_worker, RelayMode, RelayStats};

    const PIPE_CHUNK_SIZE: usize = 64 * 1024;
    const SPLICE_FLAGS: SpliceFlags = SpliceFlags::MOVE.union(SpliceFlags::NONBLOCK);
    const WOULD_BLOCK_RETRY_LIMIT: usize = 8;
    const IO_URING_RELAY_BUFFER_SIZE: usize = 32 * 1024;
    const IO_URING_WORKER_QUEUE_CAPACITY: usize = 1024;

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
        response_tx: std_mpsc::SyncSender<io::Result<RelayStats>>,
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
            let (submitter, mut receiver) = mpsc::channel::<IoUringRelayJob>(IO_URING_WORKER_QUEUE_CAPACITY);
            let (ready_tx, ready_rx) = std_mpsc::sync_channel::<io::Result<()>>(1);

            thread::Builder::new()
                .name("relay-io-uring".to_string())
                .spawn(move || {
                    tokio_uring::start(async move {
                        let _ = ready_tx.send(Ok(()));

                        while let Some(job) = receiver.recv().await {
                            tokio_uring::spawn(async move {
                                let result = run_io_uring_relay(job.client, job.upstream, job.session).await;
                                let _ = job.response_tx.send(result);
                            });
                        }
                    });
                })
                .map_err(|error| io::Error::other(format!("spawn relay-io-uring thread: {error}")))?;

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
        ) -> Result<RelayStats, IoUringRelayError> {
            let (response_tx, response_rx) = std_mpsc::sync_channel(1);
            let job = IoUringRelayJob {
                client,
                upstream,
                session,
                response_tx,
            };

            self.submitter.try_send(job).map_err(|error| match error {
                TrySendError::Full(_) => IoUringRelayError::Unavailable(io::Error::other(
                    "shared io_uring worker queue is full",
                )),
                TrySendError::Closed(_) => IoUringRelayError::Unavailable(io::Error::other(
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
    ) -> io::Result<RelayStats> {
        let client_read = client.try_clone()?;
        let client_write = client;
        let upstream_read = upstream.try_clone()?;
        let upstream_write = upstream;
        let upload_session = session.clone();
        let download_session = session;

        let upload = spawn_relay_worker("splice-upload", move || {
            let copied = splice_copy(
                &client_read,
                &upstream_write,
                &pipes.upload.read_end,
                &pipes.upload.write_end,
                &upload_session,
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
            &download_session,
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

    pub fn relay_with_io_uring(
        client: TcpStream,
        upstream: TcpStream,
        session: ConnectionSession,
    ) -> Result<RelayStats, IoUringRelayError> {
        SharedIoUringWorker::shared()
            .map_err(IoUringRelayError::Unavailable)?
            .relay(client, upstream, session)
    }

    fn splice_copy(
        src: &TcpStream,
        dst: &TcpStream,
        pipe_read: &OwnedFd,
        pipe_write: &OwnedFd,
        session: &ConnectionSession,
        upload_direction: bool,
    ) -> io::Result<u64> {
        let mut total = 0_u64;

        loop {
            let moved_into_pipe = match splice_retry(src, pipe_write, PIPE_CHUNK_SIZE)? {
                Some(written) => written,
                None => return relay_with_copy_fallback(src, dst, total, session, upload_direction),
            };

            if moved_into_pipe == 0 {
                return Ok(total);
            }

            let mut remaining = moved_into_pipe;
            while remaining > 0 {
                let moved_from_pipe = match splice_retry(pipe_read, dst, remaining)? {
                    Some(written) => written,
                    None => return relay_with_copy_fallback(src, dst, total, session, upload_direction),
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
                    unsafe { libc::poll(&mut pfd, 1, 50) };
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
    ) -> io::Result<u64> {
        let mut total = 0_u64;
        let mut buf = vec![0_u8; IO_URING_RELAY_BUFFER_SIZE];

        loop {
            let (read_result, next_buf) = reader.read(buf).await;
            buf = next_buf;
            let read = read_result?;
            if read == 0 {
                writer.shutdown(std::net::Shutdown::Write)?;
                return Ok(total);
            }


            let (write_result, next_buf) = writer.write_all(buf.slice(..read)).await;
            write_result?;
            buf = next_buf.into_inner();

            let bytes = read as u64;
            total += bytes;
            if upload_direction {
                session.add_upload(bytes);
            } else {
                session.add_download(bytes);
            }
        }
    }

    async fn run_io_uring_relay(
        client: TcpStream,
        upstream: TcpStream,
        session: ConnectionSession,
    ) -> io::Result<RelayStats> {
        let client = tokio_uring::net::TcpStream::from_std(client);
        let upstream = tokio_uring::net::TcpStream::from_std(upstream);
        let client = Arc::new(client);
        let upstream = Arc::new(upstream);
        let upload_session = session.clone();
        let download_session = session;

        let upload_client = Arc::clone(&client);
        let upload_upstream = Arc::clone(&upstream);
        let upload = tokio_uring::spawn(async move {
            io_uring_copy(upload_client, upload_upstream, upload_session, true).await
        });

        let download_bytes = io_uring_copy(upstream, client, download_session, false).await?;
        let upload_bytes = upload
            .await
            .map_err(|error| io::Error::other(format!("io_uring upload task failed: {error}")))??;

        Ok(RelayStats {
            upload_bytes,
            download_bytes,
            mode: Some(RelayMode::IoUring),
        })
    }
}

#[cfg(all(target_os = "linux", feature = "linux-accel"))]
pub(super) use imp::*;
