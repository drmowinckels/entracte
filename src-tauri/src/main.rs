// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    match argv.get(1).map(|s| s.as_str()) {
        Some("help" | "-h" | "--help") => {
            print!("{}", entracte_lib::cli::help_text());
            return;
        }
        Some("log") => {
            entracte_lib::cli::stream_log();
            return;
        }
        _ => {}
    }

    match entracte_lib::cli::parse_cli(&argv) {
        Err(e) => {
            eprintln!("entracte: {e:?}");
            eprintln!();
            eprintln!("{}", entracte_lib::cli::help_text());
            std::process::exit(2);
        }
        Ok(Some(cmd)) if cmd.runs_locally() => {
            std::process::exit(entracte_lib::cli::run_local_ipc(cmd));
        }
        _ => entracte_lib::run(),
    }
}
