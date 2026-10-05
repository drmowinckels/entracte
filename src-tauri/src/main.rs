// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use entracte_lib::cli::{self, Invocation};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let invocation = cli::classify(&argv);

    // The `windows_subsystem = "windows"` binary starts with no console, so any
    // CLI output would go to a closed handle (#364). Attach to the parent's
    // console on the paths that print — never on the tray-app launch, or
    // starting Entracte from Explorer would flash a console window.
    #[cfg(windows)]
    if invocation.produces_console_output() {
        cli::attach_parent_console();
    }

    match invocation {
        Invocation::Help => print!("{}", cli::help_text()),
        Invocation::Log => cli::stream_log(),
        Invocation::ParseError(e) => {
            eprintln!("entracte: {e:?}");
            eprintln!();
            eprintln!("{}", cli::help_text());
            std::process::exit(2);
        }
        Invocation::Local(cmd) => {
            std::process::exit(cli::run_local_ipc(cmd));
        }
        Invocation::LaunchApp => entracte_lib::run(),
    }
}
