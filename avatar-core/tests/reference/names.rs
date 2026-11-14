/// Whether an example may print names: an avatar outside this repository, and its recordings, hold names that are
/// not this repository's, so without `--names` a failure reports only where it happened and the name traces are
/// refused.
pub fn names() -> bool {
    let names = std::env::args_os().any(|a| a == "--names");
    if !names {
        for trace in ["AW_TRACE", "AW_CLOCK"] {
            if std::env::var_os(trace).is_some() {
                eprintln!("{trace} prints names: pass --names");
                std::process::exit(2);
            }
        }
        std::panic::set_hook(Box::new(|info| {
            let at = info.location().map_or(String::new(), |l| format!(" at {}:{}", l.file(), l.line()));
            eprintln!("failed{at}: pass --names for the message");
        }));
    }
    names
}

/// The command's arguments without `--names`.
pub fn args() -> Vec<String> {
    std::env::args().filter(|a| a != "--names").collect()
}
