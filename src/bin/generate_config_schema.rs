use std::path::Path;

fn main() {
    if let Err(error) = necron_prism::proxy::config::write_schema_file(Path::new(".")) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
