#[path = "../proxy/config/schema.rs"]
mod config_schema;
#[path = "../proxy/config/schema_types.rs"]
mod config_schema_types;
#[path = "../proxy/config/types.rs"]
mod config_types;

use std::path::Path;

fn main() {
    if let Err(error) = config_schema::write_schema_file(Path::new(".")) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
