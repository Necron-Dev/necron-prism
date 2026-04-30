use tokio::io::{AsyncRead, AsyncReadExt};

use smallvec::SmallVec;
use valence_protocol::decode::{PacketDecoder, PacketFrame};
use valence_protocol::var_int::VarInt;

use super::error::ProtocolError;

const INLINE_BUFFER_SIZE: usize = 16 * 1024;

pub struct PacketIo {
    decoder: PacketDecoder,
    read_buf: SmallVec<[u8; INLINE_BUFFER_SIZE]>,
}

pub struct FramedPacket {
    pub wire_len: usize,
    pub frame: PacketFrame,
}

impl Default for PacketIo {
    fn default() -> Self {
        Self::new(INLINE_BUFFER_SIZE)
    }
}

impl PacketIo {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            decoder: PacketDecoder::new(),
            read_buf: SmallVec::from_elem(0, buffer_size),
        }
    }

    pub fn queue_slice(&mut self, bytes: &[u8]) {
        self.decoder.queue_slice(bytes);
    }

    pub async fn read_frame<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        max_wire_size: usize,
    ) -> Result<FramedPacket, ProtocolError> {
        loop {
            if let Some(frame) = self
                .decoder
                .try_next_packet()
                .map_err(ProtocolError::decode)?
            {
                let packet_len = VarInt(frame.id).written_size() + frame.body.len();
                let wire_len = VarInt(packet_len as i32).written_size() + packet_len;

                if wire_len > max_wire_size {
                    return Err(ProtocolError::PacketTooLarge {
                        size: wire_len,
                        max_size: max_wire_size,
                    });
                }

                return Ok(FramedPacket { wire_len, frame });
            }

            let read = reader.read(&mut self.read_buf).await?;
            if read == 0 {
                return Err(ProtocolError::UnexpectedEof);
            }

            self.decoder.queue_slice(&self.read_buf[..read]);
        }
    }
}
