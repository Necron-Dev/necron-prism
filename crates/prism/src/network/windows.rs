use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::io::AsRawSocket;
use std::ptr;

use tokio::net::TcpStream;
use windows_sys::Win32::Networking::WinSock::{SOCKET, SOCKET_ERROR, WSAIoctl};

// windows-sys exposes WSAIoctl but not these mstcpip.h TCP_INFO bindings.
const IOC_VENDOR: u32 = 0x1800_0000;
const IOC_IN: u32 = 0x8000_0000;
const IOC_OUT: u32 = 0x4000_0000;
const SIO_TCP_INFO: u32 = IOC_IN | IOC_OUT | IOC_VENDOR | 39;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct TcpInfoV0 {
    state: i32,
    mss: u32,
    connection_time_ms: u64,
    timestamps_enabled: u8,
    rtt_us: u32,
    min_rtt_us: u32,
    bytes_in_flight: u32,
    cwnd: u32,
    snd_wnd: u32,
    rcv_wnd: u32,
    rcv_buf: u32,
    bytes_out: u64,
    bytes_in: u64,
    bytes_reordered: u32,
    bytes_retrans: u32,
    fast_retrans: u32,
    dup_acks_in: u32,
    timeout_episodes: u32,
    syn_retrans: u8,
}

pub(super) fn get_client_rtt_ms(stream: &TcpStream) -> Option<u32> {
    let mut version = 0u32;
    let mut info = TcpInfoV0::default();
    let mut bytes_returned = 0u32;

    let result = unsafe {
        WSAIoctl(
            stream.as_raw_socket() as SOCKET,
            SIO_TCP_INFO,
            &mut version as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
            &mut info as *mut TcpInfoV0 as *mut c_void,
            size_of::<TcpInfoV0>() as u32,
            &mut bytes_returned,
            ptr::null_mut(),
            None,
        )
    };

    if result == SOCKET_ERROR || bytes_returned < size_of::<TcpInfoV0>() as u32 {
        return None;
    }

    super::micros_to_millis_ceil(info.rtt_us)
}
