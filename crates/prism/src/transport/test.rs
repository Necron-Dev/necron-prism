#[cfg(test)]
mod test {
    use std::io;
    use std::net::SocketAddr;
    use std::time::Instant;
    use anyhow::Result;
    use necron_prism_minecraft::{FramedPacket, HandshakeInfo, PacketIo};

    use super::*;
    use crate::config::Config;
    use crate::hooks::LoginResult;
    use crate::{ConnectionKind, ConnectionReport, ConnectionSession, PrismContext, PrismHooks};
    use crate::transport::{finalize_connection, ConnectionOutcome};

    struct NoopHooks;

    impl PrismHooks for NoopHooks {
        async fn on_legacy_ping(
            &self,
            _client: &mut tokio::net::TcpStream,
            _session: &ConnectionSession,
            _config: &Config,
            _online_count: i32,
        ) -> std::result::Result<()> {
            Ok(())
        }

        async fn on_status_request(
            &self,
            _packet_io: &mut PacketIo,
            _client: &mut tokio::net::TcpStream,
            _session: &ConnectionSession,
            _handshake: &HandshakeInfo,
            _config: &Config,
            _online_count: i32,
        ) -> std::result::Result<()> {
            Ok(())
        }

        async fn on_login(
            &self,
            _client: &mut tokio::net::TcpStream,
            _session: &ConnectionSession,
            _handshake: &HandshakeInfo,
            _login_packet: &FramedPacket,
            _peer_addr: Option<SocketAddr>,
            _config: &Config,
            _online_count: i32,
        ) -> std::result::Result<LoginResult> {
            unreachable!()
        }

        fn on_connection_established(
            &self,
            _session: &ConnectionSession,
            _external_connection_id: &str,
            _player_name: Option<&str>,
            _player_uuid: Option<&str>,
        ) {
        }

        fn on_connection_finished(&self, _session: &ConnectionSession, _report: &ConnectionReport) {}
    }

    #[test]
    fn finalize_removes_proxy_connection_for_expected_disconnect() {
        let ctx = PrismContext::new(Config::default(), NoopHooks);
        let session = ConnectionSession::new(None);
        session.set_kind(ConnectionKind::Proxy);
        session.set_connection_id("cid-1".to_string());

        let remaining = ctx.runtime().connections.register(session.clone());
        assert_eq!(remaining, 1);

        ctx.runtime()
            .connections
            .update_outbound("cid-1", "server.example:25565".into());

        assert_eq!(ctx.runtime().connections.current_online_count(), 1);
        assert_eq!(ctx.runtime().connections.active_count(), 1);

        let outcome = ConnectionOutcome::Failed {
            report: ConnectionReport::new(session.connection_traffic(), None, None, None),
            error: anyhow::Error::new(io::Error::new(io::ErrorKind::UnexpectedEof, "eof")),
            expected_disconnect: true,
        };

        finalize_connection(&ctx, &session, Instant::now(), outcome);

        assert_eq!(ctx.runtime().connections.current_online_count(), 0);
        assert_eq!(ctx.runtime().connections.active_count(), 0);
    }

    #[test]
    fn finalize_is_safe_when_connection_was_already_removed() {
        let ctx = PrismContext::new(Config::default(), NoopHooks);
        let session = ConnectionSession::new(None);
        session.set_kind(ConnectionKind::Proxy);
        session.set_connection_id("cid-2".to_string());

        let remaining = ctx.runtime().connections.register(session.clone());
        assert_eq!(remaining, 1);

        ctx.runtime()
            .connections
            .update_outbound("cid-2", "server.example:25565".into());
        assert_eq!(ctx.runtime().connections.current_online_count(), 1);

        let remaining = ctx.runtime().connections.remove_connection("cid-2");
        assert_eq!(remaining, 0);
        assert_eq!(ctx.runtime().connections.current_online_count(), 0);

        finalize_connection(
            &ctx,
            &session,
            Instant::now(),
            ConnectionOutcome::Completed(ConnectionReport::new(
                session.connection_traffic(),
                None,
                None,
                None,
            )),
        );

        assert_eq!(ctx.runtime().connections.current_online_count(), 0);
        assert_eq!(ctx.runtime().connections.active_count(), 0);
    }
}