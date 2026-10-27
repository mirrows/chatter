fn main() {
    env_logger::init();
    if let Err(e) = chatters_lib::run() {
        eprintln!("chatters exited with error: {e}");
        std::process::exit(1);
    }
}
