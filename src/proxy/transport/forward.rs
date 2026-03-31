use tokio::io::AsyncWriteExt;

use crate::minecraft::{encode_raw_frame, FramedPacket};

pub async fn forward_login_start(
    upstream: &mut tokio::net::TcpStream,
    login_start_packet: &FramedPacket,
) -> anyhow::Result<()> {
    let encoded_login_start = encode_raw_frame(login_start_packet).map_err(anyhow::Error::from)?;
    upstream.write_all(&encoded_login_start).await?;
    Ok(())
}

pub fn compute_upload_bytes(
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
) -> u64 {
    handshake_packet.wire_len as u64 + login_start_packet.wire_len as u64
}
