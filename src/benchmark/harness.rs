use crate::proxy::config::{RelayMode, SocketOptions};
use crate::proxy::relay::relay_bidirectional;
use crate::proxy::traffic::ConnectionCounters;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::OnceLock;
use std::thread;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct RelayHarness {
    client_addr: std::net::SocketAddr,
    _upstream_server: thread::JoinHandle<()>,
    _relay_manager: thread::JoinHandle<()>,
}

#[derive(Clone, Copy, Debug)]
pub enum RelayImplementation {
    Standard,
    CustomAsync,
    Sync,
}

impl RelayHarness {
    pub fn new(buffer_size: usize) -> Self {
        Self::with_impl(buffer_size, RelayImplementation::Standard)
    }

    pub fn with_impl(buffer_size: usize, implementation: RelayImplementation) -> Self {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_server = thread::spawn(move || loop {
            if let Ok((mut stream, _)) = upstream_listener.accept() {
                stream.set_nodelay(true).ok();
                thread::spawn(move || {
                    let mut buf = vec![0u8; buffer_size];
                    while let Ok(n) = stream.read(&mut buf) {
                        if n == 0 || stream.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                });
            }
        });

        let client_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let relay_manager = thread::spawn(move || loop {
            if let Ok((client_stream, _)) = client_listener.accept() {
                let upstream_stream = TcpStream::connect(upstream_addr).unwrap();
                client_stream.set_nodelay(true).ok();
                upstream_stream.set_nodelay(true).ok();

                thread::spawn(move || {
                    let _ = run_relay_implementation(implementation, client_stream, upstream_stream);
                });
            }
        });

        Self {
            client_addr,
            _upstream_server: upstream_server,
            _relay_manager: relay_manager,
        }
    }

    pub fn run_bytes(&self, payload: &[u8], repeat: usize, read_buffer_size: usize) {
        let mut client = TcpStream::connect(self.client_addr).unwrap();
        client.set_nodelay(true).ok();

        let total_bytes = payload.len() * repeat;
        let client_recv = client.try_clone().unwrap();
        let receiver = thread::spawn(move || {
            let mut client = client_recv;
            let mut received = 0usize;
            let mut buf = vec![0u8; read_buffer_size];
            while received < total_bytes {
                match client.read(&mut buf) {
                    Ok(n) => received += n,
                    _ => break,
                }
            }
        });

        for _ in 0..repeat {
            client.write_all(payload).unwrap();
        }

        let _ = receiver.join();
        let _ = client.shutdown(Shutdown::Both);
    }
}

fn run_relay_implementation(
    implementation: RelayImplementation,
    client_stream: TcpStream,
    upstream_stream: TcpStream,
) -> std::io::Result<()> {
    match implementation {
        RelayImplementation::Standard => run_standard_relay(client_stream, upstream_stream),
        RelayImplementation::CustomAsync => run_custom_async_relay(client_stream, upstream_stream),
        RelayImplementation::Sync => run_sync_relay(client_stream, upstream_stream),
    }
}

fn run_standard_relay(client_stream: TcpStream, upstream_stream: TcpStream) -> std::io::Result<()> {
    client_stream.set_nonblocking(true)?;
    upstream_stream.set_nonblocking(true)?;

    let counters = ConnectionCounters::default();
    let socket_options = SocketOptions::default();
    relay_runtime()?.block_on(async move {
        let client = tokio::net::TcpStream::from_std(client_stream)?;
        let upstream = tokio::net::TcpStream::from_std(upstream_stream)?;
        relay_bidirectional(client, upstream, counters, &socket_options, RelayMode::Standard)
            .await
            .map(|_| ())
    })
}

fn run_custom_async_relay(
    client_stream: TcpStream,
    upstream_stream: TcpStream,
) -> std::io::Result<()> {
    client_stream.set_nonblocking(true)?;
    upstream_stream.set_nonblocking(true)?;

    relay_runtime()?.block_on(async move {
        let client = tokio::net::TcpStream::from_std(client_stream)?;
        let upstream = tokio::net::TcpStream::from_std(upstream_stream)?;
        let (mut client_read, mut client_write) = client.into_split();
        let (mut upstream_read, mut upstream_write) = upstream.into_split();

        tokio::try_join!(
            custom_async_copy(&mut client_read, &mut upstream_write),
            custom_async_copy(&mut upstream_read, &mut client_write),
        )?;

        Ok::<(), std::io::Error>(())
    })
}

fn run_sync_relay(
    mut client_stream: TcpStream,
    mut upstream_stream: TcpStream,
) -> std::io::Result<()> {
    let mut client_read = client_stream.try_clone()?;
    let mut upstream_read = upstream_stream.try_clone()?;

    let upload = thread::spawn(move || std::io::copy(&mut client_read, &mut upstream_stream));
    let _download = std::io::copy(&mut upstream_read, &mut client_stream)?;
    let _ = upload
        .join()
        .map_err(|_| std::io::Error::other("sync relay upload thread panicked"))??;
    Ok(())
}

async fn custom_async_copy(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
) -> std::io::Result<u64> {
    let mut total = 0_u64;
    let mut buf = vec![0_u8; 128 * 1024];

    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            writer.shutdown().await?;
            return Ok(total);
        }

        writer.write_all(&buf[..read]).await?;
        total += read as u64;
    }
}

fn relay_runtime() -> std::io::Result<&'static tokio::runtime::Runtime> {
    static RELAY_RUNTIME: OnceLock<std::io::Result<tokio::runtime::Runtime>> = OnceLock::new();

    RELAY_RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .map_err(|error| std::io::Error::other(format!("build bench relay runtime: {error}")))
        })
        .as_ref()
        .map_err(|error| std::io::Error::new(error.kind(), error.to_string()))
}
