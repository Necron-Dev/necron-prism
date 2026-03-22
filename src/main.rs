mod minecraft;
mod proxy;

fn main() {
    if let Err(error) = proxy::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
