use prism::config::write_schema_file;
use std::path::Path;

fn main() {
    let root = Path::new(".");
    if let Err(error) = write_schema_file(root) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
    println!("wrote config schema to {}", root.join("config.schema.json").display());
}
