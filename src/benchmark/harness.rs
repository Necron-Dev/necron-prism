#![cfg_attr(not(feature = "benchmark"), allow(dead_code, unused_imports))]

#[cfg(feature = "benchmark")]
mod imp {
    use crate::proxy::config::{RelayMode, SocketOptions};
    use crate::proxy::relay::relay_bidirectional;
    use crate::proxy::traffic::ConnectionCounters;
    use parking_lot::{Condvar, Mutex};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::thread;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone, Debug)]
    pub struct TrafficBurst {
        payload: Arc<[u8]>,
        repeat: usize,
    }

    impl TrafficBurst {
        pub fn new(payload: Vec<u8>, repeat: usize) -> Self {
            Self {
                payload: Arc::<[u8]>::from(payload),
                repeat,
            }
        }

        fn total_bytes(&self) -> u64 {
            self.payload.len() as u64 * self.repeat as u64
        }
    }

    #[derive(Clone, Debug)]
    pub struct TrafficPlan {
        label: &'static str,
        bursts: Arc<[TrafficBurst]>,
        total_bytes: u64,
    }

    impl TrafficPlan {
        pub fn new(label: &'static str, bursts: Vec<TrafficBurst>) -> Self {
            let total_bytes = bursts.iter().map(TrafficBurst::total_bytes).sum();
            Self {
                label,
                bursts: Arc::<[TrafficBurst]>::from(bursts),
                total_bytes,
            }
        }

        pub fn label(&self) -> &'static str {
            self.label
        }

        pub fn total_bytes(&self) -> u64 {
            self.total_bytes
        }
    }

    #[derive(Default)]
    struct DrainState {
        observed_bytes: Mutex<u64>,
        observed_cv: Condvar,
    }

    impl DrainState {
        fn observed(&self) -> u64 {
            *self.observed_bytes.lock()
        }

        fn record(&self, bytes: u64) {
            let mut observed = self.observed_bytes.lock();
            *observed += bytes;
            self.observed_cv.notify_all();
        }

        fn wait_for(&self, target: u64) {
            let mut observed = self.observed_bytes.lock();
            while *observed < target {
                self.observed_cv.wait(&mut observed);
            }
        }
    }

    pub struct RelayHarness {
        client: Mutex<TcpStream>,
        drain_state: Arc<DrainState>,
        upstream_server: Option<thread::JoinHandle<()>>,
        relay_manager: Option<thread::JoinHandle<()>>,
        drain_worker: Option<thread::JoinHandle<()>>,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum RelayImplementation {
        Prism,
        Sync,
        PlainCopy,
        TokioAsync,
        CustomRelay,
    }

    impl RelayHarness {
        pub fn new(buffer_size: usize) -> Self {
            Self::with_impl(buffer_size, RelayImplementation::Prism)
        }

        pub fn with_impl(buffer_size: usize, implementation: RelayImplementation) -> Self {
            let upstream_listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let upstream_addr = upstream_listener.local_addr().unwrap();
            let upstream_server = thread::spawn(move || {
                let (mut stream, _) = upstream_listener.accept().unwrap();
                stream.set_nodelay(true).ok();
                let mut buf = vec![0_u8; buffer_size];

                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            let client_listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let client_addr = client_listener.local_addr().unwrap();
            let relay_manager = thread::spawn(move || {
                let (client_stream, _) = client_listener.accept().unwrap();
                let upstream_stream = TcpStream::connect(upstream_addr).unwrap();
                client_stream.set_nodelay(true).ok();
                upstream_stream.set_nodelay(true).ok();
                let _ = run_relay_implementation(implementation, client_stream, upstream_stream);
            });

            let client = TcpStream::connect(client_addr).unwrap();
            client.set_nodelay(true).ok();
            let mut drain_stream = client.try_clone().unwrap();
            let drain_state = Arc::new(DrainState::default());
            let drain_state_for_worker = Arc::clone(&drain_state);
            let drain_worker = thread::spawn(move || {
                let mut buf = vec![0_u8; buffer_size.max(256 * 1024)];
                loop {
                    match drain_stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => drain_state_for_worker.record(n as u64),
                        Err(_) => break,
                    }
                }
            });

            Self {
                client: Mutex::new(client),
                drain_state,
                upstream_server: Some(upstream_server),
                relay_manager: Some(relay_manager),
                drain_worker: Some(drain_worker),
            }
        }

        pub fn warm_up(&self, plan: &TrafficPlan, rounds: usize) {
            for _ in 0..rounds {
                self.run_plan(plan);
            }
        }

        pub fn run_plan(&self, plan: &TrafficPlan) {
            let baseline = self.drain_state.observed();
            let target = baseline + plan.total_bytes();

            {
                let mut client = self.client.lock();
                for burst in plan.bursts.iter() {
                    for _ in 0..burst.repeat {
                        client.write_all(&burst.payload).unwrap();
                    }
                }
            }

            self.drain_state.wait_for(target);
        }
    }


    impl Drop for RelayHarness {
        fn drop(&mut self) {
            let _ = self.client.get_mut().shutdown(Shutdown::Both);

            if let Some(handle) = self.drain_worker.take() {
                let _ = handle.join();
            }
            if let Some(handle) = self.relay_manager.take() {
                let _ = handle.join();
            }
            if let Some(handle) = self.upstream_server.take() {
                let _ = handle.join();
            }
        }
    }


    fn run_relay_implementation(
        implementation: RelayImplementation,
        client_stream: TcpStream,
        upstream_stream: TcpStream,
    ) -> std::io::Result<()> {
        match implementation {
            RelayImplementation::Prism => run_prism_relay(client_stream, upstream_stream),
            RelayImplementation::Sync => run_sync_relay(client_stream, upstream_stream),
            RelayImplementation::PlainCopy => run_plain_copy_relay(client_stream, upstream_stream),
            RelayImplementation::TokioAsync => run_tokio_async_relay(client_stream, upstream_stream),
            RelayImplementation::CustomRelay => run_custom_relay(client_stream, upstream_stream),
        }
    }

    fn run_prism_relay(client_stream: TcpStream, upstream_stream: TcpStream) -> std::io::Result<()> {
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

    fn run_plain_copy_relay(
        client_stream: TcpStream,
        upstream_stream: TcpStream,
    ) -> std::io::Result<()> {
        client_stream.set_nonblocking(true)?;
        upstream_stream.set_nonblocking(true)?;

        relay_runtime()?.block_on(async move {
            let mut client = tokio::net::TcpStream::from_std(client_stream)?;
            let mut upstream = tokio::net::TcpStream::from_std(upstream_stream)?;
            tokio::io::copy_bidirectional(&mut client, &mut upstream)
                .await
                .map(|_| ())
        })
    }

    fn run_tokio_async_relay(
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

    fn run_custom_relay(client_stream: TcpStream, upstream_stream: TcpStream) -> std::io::Result<()> {
        client_stream.set_nonblocking(true)?;
        upstream_stream.set_nonblocking(true)?;

        relay_runtime()?.block_on(async move {
            let client = tokio::net::TcpStream::from_std(client_stream)?;
            let upstream = tokio::net::TcpStream::from_std(upstream_stream)?;
            let (mut client_read, mut client_write) = client.into_split();
            let (mut upstream_read, mut upstream_write) = upstream.into_split();

            tokio::try_join!(
                custom_relay_copy(&mut client_read, &mut upstream_write),
                custom_relay_copy(&mut upstream_read, &mut client_write),
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

        let upload = thread::spawn(move || {
            let copied = std::io::copy(&mut client_read, &mut upstream_stream)?;
            let _ = upstream_stream.shutdown(Shutdown::Write);
            Ok::<u64, std::io::Error>(copied)
        });
        let _download = std::io::copy(&mut upstream_read, &mut client_stream)?;
        let _ = client_stream.shutdown(Shutdown::Write);
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
        let mut buf = vec![0_u8; 256 * 1024];

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

    async fn custom_relay_copy(
        reader: &mut (impl tokio::io::AsyncRead + Unpin),
        writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    ) -> std::io::Result<u64> {
        let mut total = 0_u64;
        let mut buf = vec![0_u8; 1024 * 1024];

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
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_io()
                    .build()
                    .map_err(|error| std::io::Error::other(format!("build bench relay runtime: {error}")))
            })
            .as_ref()
            .map_err(|error| std::io::Error::new(error.kind(), error.to_string()))
    }
}

#[cfg(feature = "benchmark")]
pub use imp::*;
