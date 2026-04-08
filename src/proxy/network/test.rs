use std::io;

use super::socket::is_connect_in_progress;

#[cfg(target_os = "linux")]
#[test]
fn linux_connect_in_progress_accepts_einprogress() {
    let error = io::Error::from_raw_os_error(libc::EINPROGRESS);
    assert!(is_connect_in_progress(&error));
}

#[test]
fn connect_in_progress_accepts_would_block() {
    let error = io::Error::from(io::ErrorKind::WouldBlock);
    assert!(is_connect_in_progress(&error));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_connect_in_progress_rejects_other_errors() {
    let error = io::Error::from_raw_os_error(libc::ECONNREFUSED);
    assert!(!is_connect_in_progress(&error));
}
