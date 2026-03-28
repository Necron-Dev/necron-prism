use std::io::{self, Write};

use crate::minecraft::{encode_raw_frame, FramedPacket};

pub fn forward_login_start(
    upstream: &mut std::net::TcpStream,
    login_start_packet: &FramedPacket,
) -> io::Result<()> {
    let encoded_login_start = encode_raw_frame(login_start_packet)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    upstream.write_all(&encoded_login_start)?;

    Ok(())
}

pub fn compute_upload_bytes(
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
) -> u64 {
    handshake_packet.wire_len as u64 + login_start_packet.wire_len as u64
}
