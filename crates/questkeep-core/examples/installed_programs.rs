//! Lists the programs "Add game" offers on this PC (Apps list and Start menu), library filter
//! not applied. Dev tool: `cargo run -p questkeep-core --example installed_programs`.

#![allow(clippy::print_stdout)]

fn main() {
    let programs = questkeep_core::programs::installed_programs();
    for program in &programs {
        println!(
            "{:<40} {:?} {} {}",
            program.name,
            program.origin,
            program.executable,
            program.args.as_deref().unwrap_or_default()
        );
    }
    println!("{} programs", programs.len());
}
