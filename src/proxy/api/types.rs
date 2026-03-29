#[cfg(feature = "http-api")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct TrafficBody {
    pub send_bytes: u64,
    pub recv_bytes: u64,
}
