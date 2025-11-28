fn main() {
    if let Err(err) = shipit::cli::run() {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
