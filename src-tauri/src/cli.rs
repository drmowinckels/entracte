use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Pause(PauseTarget),
    Resume,
    Trigger(BreakKindArg),
    Skip(BreakKindArg),
    Status,
    ProfileList,
    ProfileUse(String),
    SettingsGet(String),
    SettingsSet(String, String),
    Quick {
        profile: Option<String>,
        colour: Option<String>,
    },
}

impl CliCommand {
    pub fn runs_locally(&self) -> bool {
        matches!(
            self,
            CliCommand::Pause(_)
                | CliCommand::Resume
                | CliCommand::Trigger(_)
                | CliCommand::Skip(_)
                | CliCommand::Status
                | CliCommand::ProfileList
                | CliCommand::ProfileUse(_)
                | CliCommand::SettingsGet(_)
                | CliCommand::SettingsSet(_, _)
                | CliCommand::Quick { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseTarget {
    Indefinite,
    Duration(Duration),
    UntilTomorrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakKindArg {
    Micro,
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    UnknownCommand(String),
    MissingArg(&'static str),
    InvalidDuration(String),
    InvalidKind(String),
    UnexpectedArg(String),
}

pub fn parse_cli(argv: &[String]) -> Result<Option<CliCommand>, CliError> {
    let mut args = argv.iter().skip(1);
    let Some(cmd) = args.next() else {
        return Ok(None);
    };
    if cmd.starts_with("--profile=") || cmd.starts_with("--colour=") || cmd.starts_with("--color=")
    {
        return parse_quick_flags(cmd, args).map(Some);
    }
    let parsed = match cmd.as_str() {
        "pause" => {
            let target = match args.next() {
                None => PauseTarget::Indefinite,
                Some(arg) if arg == "until-tomorrow" => PauseTarget::UntilTomorrow,
                Some(arg) => {
                    PauseTarget::Duration(parse_duration(arg).map_err(CliError::InvalidDuration)?)
                }
            };
            CliCommand::Pause(target)
        }
        "resume" => CliCommand::Resume,
        "trigger" => CliCommand::Trigger(parse_kind(args.next())?),
        "skip" => CliCommand::Skip(parse_kind(args.next())?),
        "status" => CliCommand::Status,
        "profile" => match args.next().map(|s| s.as_str()) {
            Some("list") => CliCommand::ProfileList,
            Some("use") => {
                let name = args
                    .next()
                    .ok_or(CliError::MissingArg("profile name"))?
                    .clone();
                CliCommand::ProfileUse(name)
            }
            Some(other) => return Err(CliError::UnknownCommand(format!("profile {other}"))),
            None => return Err(CliError::MissingArg("profile subcommand (list | use NAME)")),
        },
        "settings" => match args.next().map(|s| s.as_str()) {
            Some("get") => {
                let key = args
                    .next()
                    .ok_or(CliError::MissingArg("settings key"))?
                    .clone();
                CliCommand::SettingsGet(key)
            }
            Some("set") => {
                let key = args
                    .next()
                    .ok_or(CliError::MissingArg("settings key"))?
                    .clone();
                let value = args
                    .next()
                    .ok_or(CliError::MissingArg("settings value (JSON literal)"))?
                    .clone();
                CliCommand::SettingsSet(key, value)
            }
            Some(other) => return Err(CliError::UnknownCommand(format!("settings {other}"))),
            None => {
                return Err(CliError::MissingArg(
                    "settings subcommand (get KEY | set KEY VALUE)",
                ));
            }
        },
        other => return Err(CliError::UnknownCommand(other.to_string())),
    };
    expect_no_more(args)?;
    Ok(Some(parsed))
}

// Reject anything left in the argv after a command and its expected args
// were consumed. Without this, `pause 1h 30m` silently parsed as `1h` and
// dropped `30m` — confusing if you're scripting against the CLI.
fn expect_no_more<'a, I>(mut args: I) -> Result<(), CliError>
where
    I: Iterator<Item = &'a String>,
{
    if let Some(extra) = args.next() {
        let mut rest = vec![extra.clone()];
        rest.extend(args.cloned());
        return Err(CliError::UnexpectedArg(rest.join(" ")));
    }
    Ok(())
}

fn parse_quick_flags<'a>(
    first: &'a str,
    rest: impl Iterator<Item = &'a String>,
) -> Result<CliCommand, CliError> {
    let mut profile: Option<String> = None;
    let mut colour: Option<String> = None;
    let mut handle = |raw: &str| -> Result<(), CliError> {
        if let Some(v) = raw.strip_prefix("--profile=") {
            if v.is_empty() {
                return Err(CliError::MissingArg("--profile=NAME"));
            }
            profile = Some(v.to_string());
            Ok(())
        } else if let Some(v) = raw
            .strip_prefix("--colour=")
            .or_else(|| raw.strip_prefix("--color="))
        {
            if v.is_empty() {
                return Err(CliError::MissingArg("--colour=VALUE"));
            }
            colour = Some(v.to_string());
            Ok(())
        } else {
            Err(CliError::UnknownCommand(raw.to_string()))
        }
    };
    handle(first)?;
    for arg in rest {
        handle(arg.as_str())?;
    }
    Ok(CliCommand::Quick { profile, colour })
}

fn parse_kind(raw: Option<&String>) -> Result<BreakKindArg, CliError> {
    let Some(raw) = raw else {
        return Err(CliError::MissingArg("kind (micro | long)"));
    };
    match raw.to_lowercase().as_str() {
        "micro" => Ok(BreakKindArg::Micro),
        "long" => Ok(BreakKindArg::Long),
        other => Err(CliError::InvalidKind(other.to_string())),
    }
}

fn parse_duration(raw: &str) -> Result<Duration, String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err(raw.to_string());
    }
    let (num_part, unit_part): (String, String) = trimmed.chars().partition(|c| c.is_ascii_digit());
    if num_part.is_empty() {
        return Err(raw.to_string());
    }
    let n: u64 = num_part.parse().map_err(|_| raw.to_string())?;
    let secs = match unit_part.as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => n,
        "m" | "min" | "mins" | "minute" | "minutes" => {
            n.checked_mul(60).ok_or_else(|| raw.to_string())?
        }
        "h" | "hr" | "hrs" | "hour" | "hours" => {
            n.checked_mul(3600).ok_or_else(|| raw.to_string())?
        }
        _ => return Err(raw.to_string()),
    };
    Ok(Duration::from_secs(secs))
}

pub fn help_text() -> &'static str {
    "Usage: entracte [COMMAND] [ARGS]\n\
     \n\
     Action commands (forward to the running app):\n\
     \tpause [DURATION | until-tomorrow]   Pause breaks. Duration like 30m, 1h, 90, or omit for indefinite.\n\
     \tresume                              Resume scheduled breaks.\n\
     \ttrigger {micro | long}              Fire a break immediately.\n\
     \tskip    {micro | long}              Skip the next break of that kind.\n\
     \n\
     Query / mutation commands (require the app to be running, print to your terminal):\n\
     \tstatus                              Print pause state and active profile.\n\
     \tprofile list                        List profile names.\n\
     \tprofile use NAME                    Switch the active profile.\n\
     \tsettings get KEY                    Print one Settings field as JSON.\n\
     \tsettings set KEY VALUE              Update one Settings field. VALUE is a JSON literal\n\
     \t                                    (true, 1500, \"dark\", [\"foo\",\"bar\"]).\n\
     \n\
     Local commands:\n\
     \tlog                                 Print the entracte log file and follow new entries.\n\
     \thelp                                Show this help text.\n\
     \n\
     Convenience flags (combine freely, applied via IPC):\n\
     \t--profile=NAME                      Switch the active profile.\n\
     \t--colour=VALUE                      Set overlay colour. VALUE is a preset name\n\
     \t                                    (dark|midnight|forest|rose|sunset) or a hex code\n\
     \t                                    (#abc, #aabbcc). Hex flips theme to 'custom'.\n\
     \t                                    --color= is also accepted.\n\
     \n\
     With no command, launches the Entracte tray app.\n"
}

pub fn log_path() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    const BUNDLE: &str = "io.drmowinckels.entracte";
    const FILE: &str = "entracte.log";
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library/Logs")
                .join(BUNDLE)
                .join(FILE)
        })
    }
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")));
        base.map(|d| d.join(BUNDLE).join("logs").join(FILE))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join(BUNDLE).join("logs").join(FILE))
    }
}

pub fn run_local_ipc(cmd: CliCommand) -> i32 {
    use crate::ipc::{self, IpcRequest};
    let Some(data_dir) = ipc::ipc_data_dir() else {
        eprintln!("entracte: cannot resolve app data dir on this platform");
        return 1;
    };

    let requests: Vec<IpcRequest> = match &cmd {
        CliCommand::Status => vec![IpcRequest::Status],
        CliCommand::ProfileList => vec![IpcRequest::ProfileList],
        CliCommand::ProfileUse(name) => vec![IpcRequest::ProfileUse { name: name.clone() }],
        CliCommand::SettingsGet(key) => vec![IpcRequest::SettingsGet { key: key.clone() }],
        CliCommand::SettingsSet(key, raw) => {
            let value: serde_json::Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(_) => serde_json::Value::String(raw.clone()),
            };
            vec![IpcRequest::SettingsSet {
                key: key.clone(),
                value,
            }]
        }
        CliCommand::Pause(target) => {
            let duration_secs = match target {
                PauseTarget::Indefinite => None,
                PauseTarget::Duration(d) => Some(d.as_secs()),
                PauseTarget::UntilTomorrow => Some(crate::tray::seconds_until_tomorrow_morning()),
            };
            vec![IpcRequest::Pause { duration_secs }]
        }
        CliCommand::Resume => vec![IpcRequest::Resume],
        CliCommand::Trigger(kind) => vec![IpcRequest::Trigger {
            kind: match kind {
                BreakKindArg::Micro => "micro".to_string(),
                BreakKindArg::Long => "long".to_string(),
            },
        }],
        CliCommand::Skip(kind) => vec![IpcRequest::Skip {
            kind: match kind {
                BreakKindArg::Micro => "micro".to_string(),
                BreakKindArg::Long => "long".to_string(),
            },
        }],
        CliCommand::Quick { profile, colour } => {
            let mut reqs: Vec<IpcRequest> = Vec::new();
            if let Some(name) = profile {
                reqs.push(IpcRequest::ProfileUse { name: name.clone() });
            }
            if let Some(value) = colour {
                match expand_colour(value) {
                    Ok(updates) => {
                        for (key, json_value) in updates {
                            reqs.push(IpcRequest::SettingsSet {
                                key,
                                value: json_value,
                            });
                        }
                    }
                    Err(e) => {
                        eprintln!("entracte: {e}");
                        return 1;
                    }
                }
            }
            if reqs.is_empty() {
                eprintln!("entracte: no flags supplied (need --profile= and/or --colour=)");
                return 2;
            }
            reqs
        }
    };

    let mut last_ok_data: Option<serde_json::Value> = None;
    for req in &requests {
        match ipc::call(req, &data_dir) {
            Ok(resp) if resp.ok => last_ok_data = resp.data,
            Ok(resp) => {
                eprintln!(
                    "entracte: {}",
                    resp.error.unwrap_or_else(|| "unknown error".into())
                );
                return 1;
            }
            Err(e) => {
                eprintln!("entracte: {e}");
                return 1;
            }
        }
    }
    if let Some(d) = last_ok_data {
        println!("{}", serde_json::to_string_pretty(&d).unwrap_or_default());
    }
    0
}

fn expand_colour(value: &str) -> Result<Vec<(String, serde_json::Value)>, String> {
    const PRESETS: &[&str] = &["dark", "midnight", "forest", "rose", "sunset", "rotate"];
    let trimmed = value.trim();
    if PRESETS.contains(&trimmed.to_lowercase().as_str()) {
        return Ok(vec![(
            "overlay_color".to_string(),
            serde_json::Value::String(trimmed.to_lowercase()),
        )]);
    }
    if let Some(rgb_csv) = hex_to_rgb_csv(trimmed) {
        return Ok(vec![
            (
                "overlay_color".to_string(),
                serde_json::Value::String("custom".to_string()),
            ),
            (
                "overlay_custom_rgb".to_string(),
                serde_json::Value::String(rgb_csv),
            ),
        ]);
    }
    Err(format!(
        "invalid --colour value: {value:?} (expected preset name or hex #abc/#aabbcc)"
    ))
}

fn hex_to_rgb_csv(raw: &str) -> Option<String> {
    let cleaned = raw.trim().trim_start_matches('#');
    let normalized = match cleaned.len() {
        3 => cleaned
            .chars()
            .flat_map(|c| std::iter::repeat_n(c, 2))
            .collect::<String>(),
        6 => cleaned.to_string(),
        _ => return None,
    };
    if !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(&normalized, 16).ok()?;
    Some(format!(
        "{}, {}, {}",
        (n >> 16) & 0xff,
        (n >> 8) & 0xff,
        n & 0xff
    ))
}

pub fn stream_log() {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::thread;
    use std::time::Duration;

    let Some(path) = log_path() else {
        eprintln!("entracte: could not resolve log path on this platform");
        return;
    };

    let mut file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("entracte: cannot open {}: {e}", path.display());
            return;
        }
    };

    let mut buf = String::new();
    if let Err(e) = file.read_to_string(&mut buf) {
        eprintln!("entracte: error reading {}: {e}", path.display());
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(buf.as_bytes());
    let _ = stdout.flush();

    let mut pos = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return,
    };

    loop {
        thread::sleep(Duration::from_millis(500));
        let len = match std::fs::metadata(&path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if len < pos {
            pos = 0;
        }
        if len > pos {
            if file.seek(SeekFrom::Start(pos)).is_err() {
                continue;
            }
            let mut chunk = Vec::with_capacity((len - pos) as usize);
            if file.read_to_end(&mut chunk).is_err() {
                continue;
            }
            let _ = stdout.write_all(&chunk);
            let _ = stdout.flush();
            pos = len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("entracte")
            .chain(args.iter().copied())
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn no_args_returns_none() {
        assert_eq!(parse_cli(&argv(&[])).unwrap(), None);
    }

    #[test]
    fn pause_without_arg_is_indefinite() {
        assert_eq!(
            parse_cli(&argv(&["pause"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::Indefinite)),
        );
    }

    #[test]
    fn pause_with_until_tomorrow() {
        assert_eq!(
            parse_cli(&argv(&["pause", "until-tomorrow"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::UntilTomorrow)),
        );
    }

    #[test]
    fn pause_duration_parses_minutes_and_hours() {
        assert_eq!(
            parse_cli(&argv(&["pause", "30m"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::Duration(
                Duration::from_secs(1800)
            ))),
        );
        assert_eq!(
            parse_cli(&argv(&["pause", "2h"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::Duration(
                Duration::from_secs(7200)
            ))),
        );
        assert_eq!(
            parse_cli(&argv(&["pause", "45"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::Duration(
                Duration::from_secs(45)
            ))),
        );
        assert_eq!(
            parse_cli(&argv(&["pause", "10minutes"])).unwrap(),
            Some(CliCommand::Pause(PauseTarget::Duration(
                Duration::from_secs(600)
            ))),
        );
    }

    #[test]
    fn pause_with_malformed_duration_errors() {
        assert!(matches!(
            parse_cli(&argv(&["pause", "abc"])),
            Err(CliError::InvalidDuration(_))
        ));
        assert!(matches!(
            parse_cli(&argv(&["pause", "30x"])),
            Err(CliError::InvalidDuration(_))
        ));
    }

    #[test]
    fn resume_parses() {
        assert_eq!(
            parse_cli(&argv(&["resume"])).unwrap(),
            Some(CliCommand::Resume)
        );
    }

    #[test]
    fn trigger_requires_kind() {
        assert!(matches!(
            parse_cli(&argv(&["trigger"])),
            Err(CliError::MissingArg(_))
        ));
        assert_eq!(
            parse_cli(&argv(&["trigger", "micro"])).unwrap(),
            Some(CliCommand::Trigger(BreakKindArg::Micro)),
        );
        assert_eq!(
            parse_cli(&argv(&["trigger", "Long"])).unwrap(),
            Some(CliCommand::Trigger(BreakKindArg::Long)),
        );
    }

    #[test]
    fn skip_requires_kind() {
        assert!(matches!(
            parse_cli(&argv(&["skip", "weird"])),
            Err(CliError::InvalidKind(_))
        ));
        assert_eq!(
            parse_cli(&argv(&["skip", "micro"])).unwrap(),
            Some(CliCommand::Skip(BreakKindArg::Micro)),
        );
    }

    #[test]
    fn unknown_command_errors() {
        assert!(matches!(
            parse_cli(&argv(&["doomsday"])),
            Err(CliError::UnknownCommand(_))
        ));
    }

    #[test]
    fn extra_args_after_pause_duration_are_rejected() {
        // Pre-fix this silently parsed as `1h` and dropped `30m`.
        assert!(matches!(
            parse_cli(&argv(&["pause", "1h", "30m"])),
            Err(CliError::UnexpectedArg(_)),
        ));
    }

    #[test]
    fn extra_args_after_resume_are_rejected() {
        assert!(matches!(
            parse_cli(&argv(&["resume", "now"])),
            Err(CliError::UnexpectedArg(_)),
        ));
    }

    #[test]
    fn extra_args_after_trigger_kind_are_rejected() {
        assert!(matches!(
            parse_cli(&argv(&["trigger", "micro", "long"])),
            Err(CliError::UnexpectedArg(_)),
        ));
    }

    #[test]
    fn extra_args_after_settings_set_are_rejected() {
        assert!(matches!(
            parse_cli(&argv(&[
                "settings",
                "set",
                "micro_interval_secs",
                "1500",
                "extra"
            ])),
            Err(CliError::UnexpectedArg(_)),
        ));
    }

    #[test]
    fn unexpected_arg_message_includes_all_trailing_args() {
        match parse_cli(&argv(&["pause", "1h", "30m", "later"])) {
            Err(CliError::UnexpectedArg(rest)) => {
                assert!(rest.contains("30m"));
                assert!(rest.contains("later"));
            }
            other => panic!("expected UnexpectedArg, got {other:?}"),
        }
    }

    #[test]
    fn help_text_mentions_each_command() {
        let h = help_text();
        for needle in &["pause", "resume", "trigger", "skip", "log", "help"] {
            assert!(h.contains(needle), "help text missing '{needle}': {h}");
        }
    }

    #[test]
    fn quick_profile_only() {
        let out = parse_cli(&argv(&["--profile=Wellness"])).unwrap();
        match out {
            Some(CliCommand::Quick { profile, colour }) => {
                assert_eq!(profile.as_deref(), Some("Wellness"));
                assert!(colour.is_none());
            }
            _ => panic!("expected Quick variant: {out:?}"),
        }
    }

    #[test]
    fn quick_colour_with_us_spelling() {
        let out = parse_cli(&argv(&["--color=midnight"])).unwrap();
        match out {
            Some(CliCommand::Quick { profile, colour }) => {
                assert!(profile.is_none());
                assert_eq!(colour.as_deref(), Some("midnight"));
            }
            _ => panic!("expected Quick variant: {out:?}"),
        }
    }

    #[test]
    fn quick_combined_profile_and_colour() {
        let out = parse_cli(&argv(&["--profile=Focus", "--colour=#1f293a"])).unwrap();
        match out {
            Some(CliCommand::Quick { profile, colour }) => {
                assert_eq!(profile.as_deref(), Some("Focus"));
                assert_eq!(colour.as_deref(), Some("#1f293a"));
            }
            _ => panic!("expected Quick variant: {out:?}"),
        }
    }

    #[test]
    fn quick_rejects_empty_flag_value() {
        assert!(matches!(
            parse_cli(&argv(&["--profile="])),
            Err(CliError::MissingArg(_))
        ));
        assert!(matches!(
            parse_cli(&argv(&["--colour="])),
            Err(CliError::MissingArg(_))
        ));
    }

    #[test]
    fn expand_colour_preset_returns_overlay_color() {
        let out = expand_colour("midnight").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "overlay_color");
        assert_eq!(out[0].1, serde_json::Value::String("midnight".to_string()));
    }

    #[test]
    fn expand_colour_hex_three_digit_expands_to_six() {
        let out = expand_colour("#abc").unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "overlay_color");
        assert_eq!(out[0].1, serde_json::Value::String("custom".to_string()));
        assert_eq!(out[1].0, "overlay_custom_rgb");
        assert_eq!(
            out[1].1,
            serde_json::Value::String("170, 187, 204".to_string())
        );
    }

    #[test]
    fn expand_colour_hex_six_digit_with_hash() {
        let out = expand_colour("#1f293a").unwrap();
        assert_eq!(
            out[1].1,
            serde_json::Value::String("31, 41, 58".to_string())
        );
    }

    #[test]
    fn expand_colour_rejects_garbage() {
        assert!(expand_colour("not-a-colour").is_err());
        assert!(expand_colour("#zzzzzz").is_err());
        assert!(expand_colour("#abcd").is_err());
    }

    #[test]
    fn log_path_uses_bundle_subdir() {
        let p = log_path().expect("log_path resolves on the test platform");
        let s = p.to_string_lossy();
        assert!(
            s.contains("io.drmowinckels.entracte"),
            "missing bundle id in {s}"
        );
        assert!(s.ends_with("entracte.log"), "wrong filename in {s}");
    }
}
