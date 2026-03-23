#![allow(dead_code)]

mod proxy {
    pub mod config {
        #[path = "../../../proxy/config/literals.rs"]
        pub mod literals;
        pub(crate) use literals as config_literals;

        #[path = "../../../proxy/config/schema_types.rs"]
        pub mod schema_types;

        #[path = "../../../proxy/config/schema.rs"]
        pub mod schema;
    }
}

use std::path::Path;

fn main() {
    if let Err(error) = proxy::config::schema::write_schema_file(Path::new(".")) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
