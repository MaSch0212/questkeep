//! Prints what `system_info()` finds on this PC, as JSON.
#![allow(clippy::print_stdout, clippy::print_stderr)]

fn main() {
    let start = std::time::Instant::now();
    let info = questkeep_system::system_info();
    eprintln!("system_info() took {} ms", start.elapsed().as_millis());
    match serde_json::to_string_pretty(&info) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("cannot serialize: {e}"),
    }
}
