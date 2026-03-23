fn main() {
    if let Err(error) = necron_prism::proxy::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
