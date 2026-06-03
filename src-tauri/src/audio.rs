//! Native audio playback.
//!
//! Break sounds used to play through the webview's `HTMLAudioElement`. That
//! works on macOS (WKWebView) and Windows (WebView2), which decode MP3
//! natively, but not on Linux: WebKitGTK delegates media to GStreamer and
//! cannot decode MP3 without system codecs that aren't installed by default,
//! so every chime and ambient track fell silent (#114).
//!
//! Playback now runs in-process through `rodio`, which decodes with
//! Symphonia regardless of the OS or webview, and plays via CoreAudio /
//! WASAPI / ALSA. The frontend just tells us *what* to play and *when*.
//!
//! `rodio`'s device handle (`MixerDeviceSink`) and `Player`s are `!Send`, so
//! they live on one dedicated thread fed by a command channel. Everything
//! the unit tests can reach — the sound catalogue lookup and the volume
//! clamp — is pulled out as plain functions; the thread that touches the
//! audio device is the thin, untestable shim.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use rodio::Source;
use serde::Deserialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager, State};

/// How long an ambient preview plays before it is cut off. Auditions on the
/// Settings page shouldn't loop forever.
const PREVIEW_MAX: Duration = Duration::from_secs(6);

/// The bundled sound catalogue, embedded at compile time. Only `id` and
/// `file` matter here; the frontend owns the rest (titles, attribution).
#[derive(Deserialize)]
struct CatalogEntry {
    id: String,
    file: String,
}

const CATALOG_JSON: &str = include_str!("../../src/assets/sounds/credits.json");

fn catalog() -> &'static [CatalogEntry] {
    static CATALOG: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(CATALOG_JSON).unwrap_or_default())
}

/// Resolve a catalogue `sound_id` to its bundled file name, or `None` when
/// the id isn't in the catalogue. The file name is then resolved against the
/// app's resource directory by the Tauri command layer.
pub fn file_for_id(id: &str) -> Option<&'static str> {
    catalog()
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.file.as_str())
}

/// Clamp a volume to the playable `[0, 1]` range. Matches the frontend's
/// old `clampVolume` so behaviour is identical across the IPC boundary.
pub fn clamp_volume(volume: f32) -> f32 {
    volume.clamp(0.0, 1.0)
}

enum AudioCmd {
    SetVolume(f32),
    PlayOnce(PathBuf),
    StartAmbient(PathBuf),
    PreviewAmbient(PathBuf),
    StopAmbient,
    StopAll,
}

/// Handle to the audio thread. Cloneable-by-reference through Tauri state;
/// every method is fire-and-forget — a dropped thread (no output device)
/// silently swallows commands rather than erroring up the call stack.
pub struct AudioPlayer {
    tx: Mutex<Sender<AudioCmd>>,
}

impl AudioPlayer {
    /// Spawn the audio thread and return a handle to it.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = thread::Builder::new()
            .name("entracte-audio".into())
            .spawn(move || run(rx));
        Self { tx: Mutex::new(tx) }
    }

    fn send(&self, cmd: AudioCmd) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(cmd);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.send(AudioCmd::SetVolume(clamp_volume(volume)));
    }

    pub fn play_once(&self, path: PathBuf) {
        self.send(AudioCmd::PlayOnce(path));
    }

    pub fn start_ambient(&self, path: PathBuf) {
        self.send(AudioCmd::StartAmbient(path));
    }

    pub fn preview_ambient(&self, path: PathBuf) {
        self.send(AudioCmd::PreviewAmbient(path));
    }

    pub fn stop_ambient(&self) {
        self.send(AudioCmd::StopAmbient);
    }

    pub fn stop_all(&self) {
        self.send(AudioCmd::StopAll);
    }
}

/// Decode a sound file into a `rodio` source, logging (not panicking) on a
/// missing file or an unsupported codec.
fn decode(path: &Path) -> Option<rodio::Decoder<BufReader<File>>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("audio: cannot open {}: {e}", path.display());
            return None;
        }
    };
    match rodio::Decoder::try_from(file) {
        Ok(decoder) => Some(decoder),
        Err(e) => {
            log::warn!("audio: cannot decode {}: {e}", path.display());
            None
        }
    }
}

fn run(rx: Receiver<AudioCmd>) {
    let handle = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(handle) => handle,
        Err(e) => {
            log::warn!("audio: no output device, sounds disabled: {e}");
            return;
        }
    };
    let mixer = handle.mixer();
    let mut volume: f32 = 1.0;
    let mut ambient: Option<rodio::Player> = None;
    let mut one_shots: Vec<rodio::Player> = Vec::new();

    while let Ok(cmd) = rx.recv() {
        // Drop finished one-shots so the vector can't grow without bound.
        one_shots.retain(|p| !p.empty());
        match cmd {
            AudioCmd::SetVolume(v) => {
                volume = v;
                if let Some(p) = &ambient {
                    p.set_volume(volume);
                }
            }
            AudioCmd::PlayOnce(path) => {
                if let Some(source) = decode(&path) {
                    let player = rodio::Player::connect_new(mixer);
                    player.set_volume(volume);
                    player.append(source);
                    one_shots.push(player);
                }
            }
            AudioCmd::StartAmbient(path) => {
                ambient = decode(&path).map(|source| {
                    let player = rodio::Player::connect_new(mixer);
                    player.set_volume(volume);
                    player.append(source.repeat_infinite());
                    player
                });
            }
            AudioCmd::PreviewAmbient(path) => {
                ambient = decode(&path).map(|source| {
                    let player = rodio::Player::connect_new(mixer);
                    player.set_volume(volume);
                    player.append(source.repeat_infinite().take_duration(PREVIEW_MAX));
                    player
                });
            }
            AudioCmd::StopAmbient => ambient = None,
            AudioCmd::StopAll => {
                ambient = None;
                one_shots.clear();
            }
        }
    }
}

/// Resolve a bundled `sound_id` to its file inside the app's resource dir.
fn resource_sound_path(app: &AppHandle, sound_id: &str) -> Option<PathBuf> {
    let file = file_for_id(sound_id)?;
    app.path()
        .resolve(format!("sounds/{file}"), BaseDirectory::Resource)
        .ok()
}

/// A user-supplied path, or `None` for empty input — keeps the empty-string
/// short-circuit out of every custom-sound command.
fn custom_path(path: String) -> Option<PathBuf> {
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Play a one-shot once `path` has been resolved (bundled id or custom file).
/// `None` path or non-positive volume is a no-op. Shared by the bundled and
/// custom command shims so the guard logic is tested once.
fn dispatch_once(audio: &AudioPlayer, path: Option<PathBuf>, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    if let Some(path) = path {
        audio.set_volume(volume);
        audio.play_once(path);
    }
}

/// Start (or preview) an ambient loop once `path` has been resolved. `None`
/// path or non-positive volume is a no-op.
fn dispatch_ambient(audio: &AudioPlayer, path: Option<PathBuf>, volume: f32, preview: bool) {
    if volume <= 0.0 {
        return;
    }
    if let Some(path) = path {
        audio.set_volume(volume);
        if preview {
            audio.preview_ambient(path);
        } else {
            audio.start_ambient(path);
        }
    }
}

#[tauri::command]
pub fn play_sound(app: AppHandle, audio: State<'_, AudioPlayer>, sound_id: String, volume: f32) {
    dispatch_once(&audio, resource_sound_path(&app, &sound_id), volume);
}

#[tauri::command]
pub fn play_custom_sound(audio: State<'_, AudioPlayer>, path: String, volume: f32) {
    dispatch_once(&audio, custom_path(path), volume);
}

#[tauri::command]
pub fn start_ambient(app: AppHandle, audio: State<'_, AudioPlayer>, sound_id: String, volume: f32) {
    dispatch_ambient(&audio, resource_sound_path(&app, &sound_id), volume, false);
}

#[tauri::command]
pub fn start_custom_ambient(audio: State<'_, AudioPlayer>, path: String, volume: f32) {
    dispatch_ambient(&audio, custom_path(path), volume, false);
}

#[tauri::command]
pub fn preview_ambient(
    app: AppHandle,
    audio: State<'_, AudioPlayer>,
    sound_id: String,
    volume: f32,
) {
    dispatch_ambient(&audio, resource_sound_path(&app, &sound_id), volume, true);
}

#[tauri::command]
pub fn preview_custom_ambient(audio: State<'_, AudioPlayer>, path: String, volume: f32) {
    dispatch_ambient(&audio, custom_path(path), volume, true);
}

#[tauri::command]
pub fn stop_ambient(audio: State<'_, AudioPlayer>) {
    audio.stop_ambient();
}

#[tauri::command]
pub fn stop_all_sounds(audio: State<'_, AudioPlayer>) {
    audio.stop_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_is_non_empty() {
        assert!(!catalog().is_empty());
    }

    #[test]
    fn file_for_known_id_resolves() {
        // Temple bell — the default end chime.
        let file = file_for_id("337048").expect("known id resolves");
        assert!(file.ends_with(".mp3"), "got {file}");
    }

    #[test]
    fn file_for_unknown_id_is_none() {
        assert_eq!(file_for_id("not-a-real-id"), None);
        assert_eq!(file_for_id(""), None);
    }

    #[test]
    fn clamp_volume_bounds_to_unit_range() {
        assert_eq!(clamp_volume(-0.5), 0.0);
        assert_eq!(clamp_volume(0.0), 0.0);
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(1.0), 1.0);
        assert_eq!(clamp_volume(2.0), 1.0);
    }

    #[test]
    fn decode_missing_file_is_none() {
        assert!(decode(Path::new("/no/such/sound.mp3")).is_none());
    }

    #[test]
    fn decode_bundled_mp3_succeeds() {
        // Proves the Symphonia MP3 decoder is wired up: decoding never
        // touches an audio device, so this is safe in headless CI.
        let file = file_for_id("337048").expect("known id");
        let path = format!("{}/../src/assets/sounds/{file}", env!("CARGO_MANIFEST_DIR"));
        assert!(
            decode(Path::new(&path)).is_some(),
            "failed to decode bundled mp3 at {path}"
        );
    }

    #[test]
    fn custom_path_empty_is_none_otherwise_some() {
        assert_eq!(custom_path(String::new()), None);
        assert_eq!(
            custom_path("/tmp/a.wav".into()),
            Some(PathBuf::from("/tmp/a.wav"))
        );
    }

    // The command dispatch helpers and the `AudioPlayer` handle only enqueue
    // commands; they never block on the audio device. In headless CI the
    // device fails to open and the thread exits, so sends are dropped — but
    // nothing panics, which is all these guard-coverage tests assert. A
    // missing path keeps the player untouched.
    const ABSENT: &str = "/no/such/audio/file.mp3";

    #[test]
    fn dispatch_once_covers_guard_and_action() {
        let audio = AudioPlayer::spawn();
        dispatch_once(&audio, Some(PathBuf::from(ABSENT)), 0.6);
        dispatch_once(&audio, None, 0.6);
        dispatch_once(&audio, Some(PathBuf::from(ABSENT)), 0.0);
    }

    #[test]
    fn dispatch_ambient_covers_start_and_preview() {
        let audio = AudioPlayer::spawn();
        dispatch_ambient(&audio, Some(PathBuf::from(ABSENT)), 0.6, false);
        dispatch_ambient(&audio, Some(PathBuf::from(ABSENT)), 0.6, true);
        dispatch_ambient(&audio, None, 0.6, false);
        dispatch_ambient(&audio, Some(PathBuf::from(ABSENT)), 0.0, true);
    }

    #[test]
    fn audio_player_methods_do_not_panic() {
        let audio = AudioPlayer::spawn();
        audio.set_volume(0.5);
        audio.set_volume(5.0);
        audio.play_once(PathBuf::from(ABSENT));
        audio.start_ambient(PathBuf::from(ABSENT));
        audio.preview_ambient(PathBuf::from(ABSENT));
        audio.stop_ambient();
        audio.stop_all();
    }
}
