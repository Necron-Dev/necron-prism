mod socket;

#[cfg(test)]
mod test;

pub use socket::{apply_sockref_options, connect_stream, create_listener};
