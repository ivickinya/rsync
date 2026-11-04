use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::profiles::{ProfileStore, ServerProfile};

const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_SOURCES: usize = 100;
const MAX_EXCLUDES: usize = 100;
const MAX_COMPARE_FILE_BYTES: u64 = 1024 * 1024;
const WATCH_DEBOUNCE: Duration = Duration::from_millis(500);
const WATCH_POLL: Duration = Duration::from_millis(100);
const PROTECTED_PATHS: &[&str] = &[
    ".env",
    ".env.*",
    "config/production.php",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "*.crt",
    "*.cer",
    "*.cert",
    "*.p8",
    "*.der",
    "*.jks",
    "*.secret",
    "*.jwt",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    profile_id: String,
    direction: String,
    sources: Vec<String>,
    destination: String,
    excludes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    id: String,
    output: String,
    source_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    output: String,
    source_count: usize,
    completed_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDirectoryEntry {
    name: String,
    path: String,
    is_directory: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDirectoryListing {
    path: String,
    entries: Vec<RemoteDirectoryEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareFileInput {
    profile_id: String,
    direction: String,
    source: String,
    destination: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareFileResult {
    local_path: String,
    remote_path: String,
    local_content: Option<String>,
    remote_content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteFileContent {
    exists: bool,
    content: Option<String>,
    error: Option<String>,
}

struct PendingTransfer {
    profile: ServerProfile,
    args: Vec<String>,
    created_at: Instant,
    source_count: usize,
    watch_root: Option<PathBuf>,
}

#[derive(Default)]
pub struct TransferStore {
    pending: Mutex<HashMap<String, PendingTransfer>>,
}

#[derive(Clone, Default)]
pub struct WatchStore {
    active: Arc<Mutex<Option<WatchSession>>>,
}

struct WatchSession {
    id: String,
    profile_id: String,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchStatus {
    watcher_id: String,
    state: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchStartResult {
    watcher_id: String,
    output: String,
}

impl TransferStore {
    fn add_preview(
        &self,
        transfer: PendingTransfer,
        output: String,
    ) -> Result<PreviewResult, String> {
        let id = Uuid::new_v4().to_string();
        let source_count = transfer.source_count;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Transfer preview storage is unavailable.".to_string())?;
        pending.retain(|_, item| item.created_at.elapsed() < PREVIEW_TTL);
        pending.insert(id.clone(), transfer);
        Ok(PreviewResult {
            id,
            output,
            source_count,
        })
    }

    fn take_preview(&self, id: &str) -> Result<PendingTransfer, String> {
        let transfer = self
            .pending
            .lock()
            .map_err(|_| "Transfer preview storage is unavailable.".to_string())?
            .remove(id)
            .ok_or_else(|| {
                "This preview has expired or was already used. Preview again.".to_string()
            })?;

        if transfer.created_at.elapsed() >= PREVIEW_TTL {
            return Err("This preview has expired. Preview again before syncing.".to_string());
        }

        Ok(transfer)
    }
}

impl WatchStore {
    fn reserve(
        &self,
        id: String,
        profile_id: String,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Directory watcher state is unavailable.".to_string())?;
        if active.is_some() {
            return Err("Stop the active watch session before starting another.".to_string());
        }
        *active = Some(WatchSession {
            id,
            profile_id,
            cancelled,
        });
        Ok(())
    }

    fn stop(&self, id: &str) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Directory watcher state is unavailable.".to_string())?;
        let session = active
            .as_ref()
            .filter(|session| session.id == id)
            .ok_or_else(|| "This watch session is no longer active.".to_string())?;
        session.cancelled.store(true, Ordering::Release);
        Ok(())
    }

    pub fn stop_for_profile(&self, profile_id: &str) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Directory watcher state is unavailable.".to_string())?;
        if let Some(session) = active
            .as_ref()
            .filter(|session| session.profile_id == profile_id)
        {
            session.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub fn stop_active(&self) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Directory watcher state is unavailable.".to_string())?;
        if let Some(session) = active.as_ref() {
            session.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn clear(&self, id: &str) {
        match self.active.lock() {
            Ok(mut active) => {
                if active.as_ref().is_some_and(|session| session.id == id) {
                    *active = None;
                }
            }
            Err(error) => log::error!("Could not clear stopped watcher state: {error}"),
        }
    }
}

impl Drop for WatchStore {
    fn drop(&mut self) {
        match self.active.lock() {
            Ok(active) => {
                if let Some(session) = active.as_ref() {
                    session.cancelled.store(true, Ordering::Release);
                }
            }
            Err(error) => {
                log::error!("Could not stop watcher during application shutdown: {error}")
            }
        }
    }
}

#[tauri::command]
pub async fn test_ssh_connection(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
) -> Result<String, String> {
    let profile = profiles.get(&profile_id)?;
    tauri::async_runtime::spawn_blocking(move || test_connection(&profile))
        .await
        .map_err(|error| format!("Could not test SSH connection: {error}"))?
}

#[tauri::command]
pub async fn open_ssh_terminal(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
) -> Result<(), String> {
    let profile = profiles.get(&profile_id)?;
    tauri::async_runtime::spawn_blocking(move || launch_ssh_terminal(&profile))
        .await
        .map_err(|error| format!("Could not open SSH terminal: {error}"))?
}

fn launch_ssh_terminal(profile: &ServerProfile) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut ssh_args = vec![
            "ssh".to_string(),
            "-p".to_string(),
            profile.port.to_string(),
            "-oConnectTimeout=10".to_string(),
            format!("{}@{}", profile.username, profile.host),
        ];
        if let Some(key_path) = &profile.key_path {
            ssh_args.splice(1..1, ["-i".to_string(), key_path.clone()]);
        }
        let shell_command = ssh_args
            .iter()
            .map(|argument| shell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ");
        let apple_script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            shell_command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let output = Command::new("osascript")
            .args(["-e", &apple_script])
            .output()
            .map_err(|error| format!("Could not launch Terminal: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = String::from_utf8_lossy(&output.stderr);
            Err(if detail.trim().is_empty() {
                format!("Terminal launch failed with status {}.", output.status)
            } else {
                detail.trim().to_string()
            })
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = profile;
        Err("Opening an SSH terminal is currently supported on macOS only.".to_string())
    }
}

#[tauri::command]
pub async fn list_remote_directory(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
    root_path: String,
    path: String,
) -> Result<RemoteDirectoryListing, String> {
    let profile = profiles.get(&profile_id)?;
    validate_remote_path(&root_path)?;
    validate_remote_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || list_directory(&profile, &root_path, &path))
        .await
        .map_err(|error| format!("Could not list remote folder contents: {error}"))?
}

#[tauri::command]
pub async fn compare_transfer_file(
    profiles: State<'_, ProfileStore>,
    input: CompareFileInput,
) -> Result<CompareFileResult, String> {
    let profile = profiles.get(&input.profile_id)?;
    tauri::async_runtime::spawn_blocking(move || compare_file(&profile, &input))
        .await
        .map_err(|error| format!("Could not compare files: {error}"))?
}

#[tauri::command]
pub async fn preview_rsync(
    profiles: State<'_, ProfileStore>,
    transfers: State<'_, TransferStore>,
    input: TransferInput,
) -> Result<PreviewResult, String> {
    let profile = profiles.get(&input.profile_id)?;
    let (preview_args, source_count) = build_rsync_args(&profile, &input, true)?;
    let (live_args, _) = build_rsync_args(&profile, &input, false)?;
    let watch_root = watch_root_for_input(&input)?;
    let output = tauri::async_runtime::spawn_blocking(move || run_rsync(&preview_args))
        .await
        .map_err(|error| format!("Could not create transfer preview: {error}"))??;

    transfers.add_preview(
        PendingTransfer {
            profile,
            args: live_args,
            created_at: Instant::now(),
            source_count,
            watch_root,
        },
        output,
    )
}

#[tauri::command]
pub async fn start_directory_watcher(
    app: tauri::AppHandle,
    profiles: State<'_, ProfileStore>,
    transfers: State<'_, TransferStore>,
    watchers: State<'_, WatchStore>,
    preview_id: String,
) -> Result<WatchStartResult, String> {
    let pending = transfers.take_preview(&preview_id)?;
    let root = pending.watch_root.clone().ok_or_else(|| {
        "Watch & Sync requires one local folder as the transfer source.".to_string()
    })?;
    if profiles.get(&pending.profile.id)? != pending.profile {
        return Err(
            "The server profile changed after this preview. Preview again before syncing."
                .to_string(),
        );
    }

    let watcher_id = Uuid::new_v4().to_string();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = sender.send(event);
        },
        Config::default(),
    )
    .map_err(|error| format!("Could not create the local file watcher: {error}"))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|error| format!("Could not watch the selected folder: {error}"))?;

    watchers.reserve(
        watcher_id.clone(),
        pending.profile.id.clone(),
        cancelled.clone(),
    )?;
    emit_watch_status(
        &app,
        &watcher_id,
        "starting",
        "Syncing the approved preview and starting the folder watcher…",
    );

    let initial_args = pending.args.clone();
    let output = match tauri::async_runtime::spawn_blocking(move || run_rsync(&initial_args)).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            watchers.clear(&watcher_id);
            emit_watch_status(&app, &watcher_id, "stopped", &error);
            return Err(error);
        }
        Err(error) => {
            let message = format!("Could not start the initial watch sync: {error}");
            watchers.clear(&watcher_id);
            emit_watch_status(&app, &watcher_id, "stopped", &message);
            return Err(message);
        }
    };

    let worker_args = pending.args;
    let worker_app = app.clone();
    let worker_watchers = watchers.inner().clone();
    let worker_id = watcher_id.clone();
    let worker_cancelled = cancelled.clone();
    if let Err(error) = thread::Builder::new()
        .name("rsync-directory-watcher".to_string())
        .spawn(move || {
            run_watch_loop(
                worker_app,
                worker_watchers,
                worker_id,
                worker_cancelled,
                watcher,
                receiver,
                root,
                worker_args,
            )
        })
    {
        let message =
            format!("The initial sync completed, but the watcher could not start: {error}");
        watchers.clear(&watcher_id);
        emit_watch_status(&app, &watcher_id, "stopped", &message);
        return Err(message);
    }

    Ok(WatchStartResult { watcher_id, output })
}

#[tauri::command]
pub fn stop_directory_watcher(
    watchers: State<'_, WatchStore>,
    watcher_id: String,
) -> Result<(), String> {
    watchers.stop(&watcher_id)
}

fn watch_root_for_input(input: &TransferInput) -> Result<Option<PathBuf>, String> {
    if input.direction != "push" || input.sources.len() != 1 {
        return Ok(None);
    }

    let source = input.sources[0].trim();
    let path = Path::new(source);
    if !path.is_absolute() {
        return Ok(None);
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Could not access source path \"{source}\": {error}"))?;
    Ok(canonical.is_dir().then_some(canonical))
}

#[allow(clippy::too_many_arguments)]
fn run_watch_loop(
    app: tauri::AppHandle,
    watchers: WatchStore,
    watcher_id: String,
    cancelled: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
    root: PathBuf,
    args: Vec<String>,
) {
    emit_watch_status(
        &app,
        &watcher_id,
        "watching",
        &format!("Watching {} for changes.", root.display()),
    );
    let mut changed_at = None;
    let mut terminal_message = "Watch & Sync stopped.".to_string();

    loop {
        if cancelled.load(Ordering::Acquire) {
            break;
        }

        let timeout = changed_at
            .map(|last_change: Instant| {
                WATCH_DEBOUNCE
                    .saturating_sub(last_change.elapsed())
                    .min(WATCH_POLL)
            })
            .unwrap_or(WATCH_POLL);

        match receiver.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                if event.paths.iter().any(|path| path.starts_with(&root)) {
                    changed_at = Some(Instant::now());
                    emit_watch_status(
                        &app,
                        &watcher_id,
                        "pending",
                        "Changes detected; waiting for file writes to settle…",
                    );
                }
            }
            Ok(Err(error)) => {
                terminal_message = format!("File watcher stopped after an error: {error}");
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                terminal_message = "File watcher stopped unexpectedly.".to_string();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if changed_at.is_some_and(|last_change| last_change.elapsed() >= WATCH_DEBOUNCE) {
            changed_at = None;
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            emit_watch_status(
                &app,
                &watcher_id,
                "syncing",
                "Syncing changes to the server…",
            );
            match run_rsync(&args) {
                Ok(_) => emit_watch_status(
                    &app,
                    &watcher_id,
                    "watching",
                    "Changes synced. Watching for more.",
                ),
                Err(error) => emit_watch_status(
                    &app,
                    &watcher_id,
                    "error",
                    &format!("Automatic sync failed; watching will continue: {error}"),
                ),
            }
        }
    }

    watchers.clear(&watcher_id);
    emit_watch_status(&app, &watcher_id, "stopped", &terminal_message);
}

fn emit_watch_status(app: &tauri::AppHandle, watcher_id: &str, state: &str, message: &str) {
    if let Err(error) = app.emit(
        "watch-status",
        WatchStatus {
            watcher_id: watcher_id.to_string(),
            state: state.to_string(),
            message: message.to_string(),
        },
    ) {
        log::warn!("Could not publish watch status: {error}");
    }
}

#[tauri::command]
pub async fn execute_rsync(
    profiles: State<'_, ProfileStore>,
    transfers: State<'_, TransferStore>,
    preview_id: String,
) -> Result<TransferResult, String> {
    let pending = transfers.take_preview(&preview_id)?;
    if profiles.get(&pending.profile.id)? != pending.profile {
        return Err(
            "The server profile changed after this preview. Preview again before syncing."
                .to_string(),
        );
    }

    let source_count = pending.source_count;
    let output = tauri::async_runtime::spawn_blocking(move || run_rsync(&pending.args))
        .await
        .map_err(|error| format!("Could not start transfer: {error}"))??;

    Ok(TransferResult {
        output,
        source_count,
        completed_at: chrono_like_timestamp(),
    })
}

fn build_rsync_args(
    profile: &ServerProfile,
    input: &TransferInput,
    dry_run: bool,
) -> Result<(Vec<String>, usize), String> {
    let direction = input.direction.as_str();
    if !matches!(direction, "push" | "pull") {
        return Err("Choose a valid transfer direction.".to_string());
    }

    let sources = input
        .sources
        .iter()
        .map(|source| source.trim())
        .filter(|source| !source.is_empty())
        .collect::<Vec<_>>();
    if sources.is_empty() || sources.len() > MAX_SOURCES {
        return Err(format!(
            "Choose between 1 and {MAX_SOURCES} source files or folders."
        ));
    }
    if input.destination.trim().is_empty() {
        return Err("Choose a destination folder.".to_string());
    }
    if input.excludes.len() > MAX_EXCLUDES {
        return Err(format!(
            "Choose no more than {MAX_EXCLUDES} exclusion patterns."
        ));
    }

    let (local_sources, remote_sources, local_destination, remote_destination) =
        if direction == "push" {
            (
                canonical_sources(&sources)?,
                Vec::new(),
                None,
                validate_remote_path(&input.destination)?,
            )
        } else {
            (
                Vec::new(),
                sources
                    .iter()
                    .map(|source| {
                        let source = validate_remote_path(source)?;
                        if is_protected_path(&source) {
                            return Err(format!(
                                "Protected files cannot be selected for transfer: \"{source}\"."
                            ));
                        }
                        Ok(source)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                Some(validate_local_directory(&input.destination)?),
                String::new(),
            )
        };

    let mut args = vec![
        "--archive".to_string(),
        "--human-readable".to_string(),
        "--itemize-changes".to_string(),
        "--progress".to_string(),
        "--protect-args".to_string(),
        "--out-format=%i %n".to_string(),
        "--rsh".to_string(),
        ssh_remote_shell(profile),
    ];
    if dry_run {
        args.push("--dry-run".to_string());
    }
    for exclude in &input.excludes {
        let pattern = exclude.trim();
        if !pattern.is_empty() {
            validate_exclude(pattern)?;
            args.push(format!("--exclude={pattern}"));
        }
    }
    args.extend(
        PROTECTED_PATHS
            .iter()
            .map(|pattern| format!("--exclude={pattern}")),
    );

    args.push("--".to_string());
    if direction == "push" {
        args.extend(
            local_sources
                .into_iter()
                .map(|path| path.to_string_lossy().to_string()),
        );
        args.push(format!(
            "{}@{}:{}",
            profile.username, profile.host, remote_destination
        ));
    } else {
        args.extend(
            remote_sources
                .into_iter()
                .map(|path| format!("{}@{}:{}", profile.username, profile.host, path)),
        );
        args.push(
            local_destination
                .expect("pull destination was validated")
                .to_string_lossy()
                .to_string(),
        );
    }

    Ok((args, sources.len()))
}

fn canonical_sources(sources: &[&str]) -> Result<Vec<PathBuf>, String> {
    sources
        .iter()
        .map(|source| {
            let path = Path::new(source);
            if !path.is_absolute() {
                return Err("Choose absolute paths for local source files and folders.".to_string());
            }
            let canonical = path
                .canonicalize()
                .map_err(|error| format!("Could not access source path \"{source}\": {error}"))?;
            if is_protected_path(&canonical.to_string_lossy()) {
                return Err(format!(
                    "Protected files cannot be selected for transfer: \"{source}\"."
                ));
            }
            if !canonical.is_file() && !canonical.is_dir() {
                return Err(format!(
                    "Source path is not a file or folder: \"{source}\"."
                ));
            }
            Ok(canonical)
        })
        .collect()
}

fn is_protected_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let components = normalized.split('/').collect::<Vec<_>>();
    let name = components
        .last()
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == ".env"
        || name == ".envrc"
        || name.starts_with(".env.")
        || [
            ".pem", ".key", ".p12", ".pfx", ".crt", ".cer", ".cert", ".p8", ".der", ".jks",
            ".secret", ".jwt",
        ]
        .iter()
        .any(|extension| name.ends_with(extension))
        || components.windows(2).any(|pair| {
            pair[0].eq_ignore_ascii_case("config") && pair[1].eq_ignore_ascii_case("production.php")
        })
}

fn validate_local_directory(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path.trim());
    if !path.is_absolute() {
        return Err("Choose an absolute path for the local destination folder.".to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Could not access destination folder: {error}"))?;
    if !canonical.is_dir() {
        return Err("The local destination must be an existing folder.".to_string());
    }
    Ok(canonical)
}

fn validate_remote_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('-')
        || path
            .chars()
            .any(|character| character == '\0' || character.is_control())
        || !(path.starts_with('/') || path.starts_with("~/"))
    {
        return Err(
            "Remote paths must be absolute (start with / or ~/) and contain no control characters."
                .to_string(),
        );
    }
    Ok(path.to_string())
}

fn validate_exclude(pattern: &str) -> Result<(), String> {
    if pattern.len() > 1024 || pattern.chars().any(|character| character == '\0') {
        return Err(
            "Exclude patterns must be 1024 characters or fewer and contain no null bytes."
                .to_string(),
        );
    }
    Ok(())
}

fn ssh_remote_shell(profile: &ServerProfile) -> String {
    let mut parts = vec![
        "ssh".to_string(),
        "-p".to_string(),
        profile.port.to_string(),
        "-oBatchMode=yes".to_string(),
        "-oConnectTimeout=10".to_string(),
    ];
    if let Some(key_path) = &profile.key_path {
        parts.push("-i".to_string());
        parts.push(key_path.clone());
    }
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn canonical_local_file(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Choose an absolute path for the local file.".to_string());
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Could not access local file: {error}"))?;
    if !canonical.is_file() {
        return Err("Choose an individual file to compare, not a folder.".to_string());
    }
    Ok(canonical)
}

fn join_remote_file(destination: &str, file_name: &str) -> Result<String, String> {
    let destination = validate_remote_path(destination)?;
    if file_name.is_empty() || file_name == "." || file_name == ".." || file_name.contains('/') {
        return Err("Could not determine a safe remote file name.".to_string());
    }
    validate_remote_path(&format!(
        "{}/{}",
        destination.trim_end_matches('/'),
        file_name
    ))
}

fn remote_basename(path: &str) -> Result<&str, String> {
    if path.ends_with('/') {
        return Err("Choose a remote file path, not a folder path.".to_string());
    }
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." {
        return Err("Choose a remote file path, not a folder path.".to_string());
    }
    Ok(name)
}

fn read_local_text(path: &Path) -> Result<String, String> {
    read_local_bytes(path)?
        .map(|bytes| decode_text_file(&bytes))
        .transpose()?
        .ok_or_else(|| "The selected local file could not be found.".to_string())
}

fn read_optional_local_text(path: &Path) -> Result<Option<String>, String> {
    read_local_bytes(path)?
        .map(|bytes| decode_text_file(&bytes))
        .transpose()
}

fn read_local_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect local file: {error}")),
    };
    if !metadata.is_file() {
        return Err("Choose an individual file to compare, not a folder.".to_string());
    }
    if metadata.len() > MAX_COMPARE_FILE_BYTES {
        return Err("File comparison is limited to 1 MiB per file.".to_string());
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("Could not read local file: {error}"))?;
    if bytes.len() as u64 > MAX_COMPARE_FILE_BYTES {
        return Err("File comparison is limited to 1 MiB per file.".to_string());
    }
    Ok(Some(bytes))
}

fn decode_text_file(bytes: &[u8]) -> Result<String, String> {
    if bytes.contains(&0) {
        return Err("Binary files cannot be compared as text.".to_string());
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| "Only UTF-8 text files can be compared.".to_string())
}

fn read_remote_text(profile: &ServerProfile, path: &str) -> Result<Option<String>, String> {
    const SCRIPT: &str = concat!(
        "import json,os,sys\n",
        "p=os.path.expanduser(sys.argv[1]); r={'exists':False}\n",
        "if os.path.isfile(p):\n",
        " if os.path.getsize(p)>1048576: r['error']='File comparison is limited to 1 MiB per file.'\n",
        " else:\n",
        "  with open(p,'rb') as f: b=f.read(1048577)\n",
        "  if len(b)>1048576: r['error']='File comparison is limited to 1 MiB per file.'\n",
        "  elif b'\\0' in b: r['error']='Binary files cannot be compared as text.'\n",
        "  else:\n",
        "   try: r.update({'exists':True,'content':b.decode('utf-8')})\n",
        "   except UnicodeDecodeError: r['error']='Only UTF-8 text files can be compared.'\n",
        "print(json.dumps(r,ensure_ascii=True))\n"
    );
    let remote_command = format!("python3 -c {} {}", shell_quote(SCRIPT), shell_quote(path));
    let mut command = Command::new("ssh");
    command
        .arg("-oBatchMode=yes")
        .arg("-oConnectTimeout=10")
        .arg("-p")
        .arg(profile.port.to_string());
    if let Some(key_path) = &profile.key_path {
        command.arg("-i").arg(key_path);
    }
    command
        .arg(format!("{}@{}", profile.username, profile.host))
        .arg(remote_command);

    let output = command
        .output()
        .map_err(|error| format!("Could not start SSH: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(if detail.trim().is_empty() {
            format!(
                "Remote file comparison failed with status {}.",
                output.status
            )
        } else {
            detail.trim().to_string()
        });
    }
    let response: RemoteFileContent = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not parse remote file contents: {error}"))?;
    if let Some(error) = response.error {
        return Err(error);
    }
    if response.exists && response.content.is_none() {
        return Err("The remote file did not return any text content.".to_string());
    }
    Ok(response.content)
}

fn compare_file(
    profile: &ServerProfile,
    input: &CompareFileInput,
) -> Result<CompareFileResult, String> {
    match input.direction.as_str() {
        "push" => {
            let local_path = canonical_local_file(&input.source)?;
            let file_name = local_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "Could not determine the selected local file name.".to_string())?;
            let remote_path = join_remote_file(&input.destination, file_name)?;
            Ok(CompareFileResult {
                local_path: local_path.to_string_lossy().to_string(),
                remote_path: remote_path.clone(),
                local_content: Some(read_local_text(&local_path)?),
                remote_content: read_remote_text(profile, &remote_path)?,
            })
        }
        "pull" => {
            let remote_path = validate_remote_path(&input.source)?;
            let file_name = remote_basename(&remote_path)?;
            let local_directory = validate_local_directory(&input.destination)?;
            let local_path = local_directory.join(file_name);
            let remote_content = read_remote_text(profile, &remote_path)?.ok_or_else(|| {
                "The selected remote source file could not be found or is not a regular file."
                    .to_string()
            })?;
            Ok(CompareFileResult {
                local_path: local_path.to_string_lossy().to_string(),
                remote_path,
                local_content: read_optional_local_text(&local_path)?,
                remote_content: Some(remote_content),
            })
        }
        _ => Err("Choose a valid transfer direction.".to_string()),
    }
}

fn test_connection(profile: &ServerProfile) -> Result<String, String> {
    let mut command = Command::new("ssh");
    command
        .arg("-oBatchMode=yes")
        .arg("-oConnectTimeout=10")
        .arg("-p")
        .arg(profile.port.to_string());
    if let Some(key_path) = &profile.key_path {
        command.arg("-i").arg(key_path);
    }
    command.arg(format!("{}@{}", profile.username, profile.host));
    command.arg("true");

    let output = command
        .output()
        .map_err(|error| format!("Could not start SSH: {error}"))?;
    output_message(output, "SSH connection succeeded.")
}

fn list_directory(
    profile: &ServerProfile,
    root_path: &str,
    path: &str,
) -> Result<RemoteDirectoryListing, String> {
    let script = concat!(
        "import json,os,sys; ",
        "root=os.path.realpath(os.path.expanduser(sys.argv[1])); ",
        "path=os.path.realpath(os.path.expanduser(sys.argv[2])); ",
        "inside=os.path.commonpath((root,path))==root; ",
        "assert inside, 'You can only browse within the selected remote folder.'; ",
        "assert os.path.isdir(path), 'The remote path is not a folder.'; ",
        "entries=[]; ",
        "names=os.listdir(path); ",
        "entries=[{'name':name,'path':os.path.join(path,name),",
        "'isDirectory':os.path.isdir(os.path.join(path,name)) and not os.path.islink(os.path.join(path,name))} ",
        "for name in names if os.path.isdir(os.path.join(path,name)) or os.path.isfile(os.path.join(path,name))]; ",
        "entries.sort(key=lambda item:(not item['isDirectory'],item['name'].casefold(),item['name'])); ",
        "print(json.dumps({'path':path,'entries':entries},ensure_ascii=True))"
    );
    let remote_command = format!(
        "python3 -c {} {} {}",
        shell_quote(script),
        shell_quote(root_path),
        shell_quote(path)
    );
    let mut command = Command::new("ssh");
    command
        .arg("-oBatchMode=yes")
        .arg("-oConnectTimeout=10")
        .arg("-p")
        .arg(profile.port.to_string());
    if let Some(key_path) = &profile.key_path {
        command.arg("-i").arg(key_path);
    }
    command
        .arg(format!("{}@{}", profile.username, profile.host))
        .arg(remote_command);

    let output = command
        .output()
        .map_err(|error| format!("Could not start SSH: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(if detail.trim().is_empty() {
            format!(
                "Remote folder listing failed with status {}.",
                output.status
            )
        } else {
            detail.trim().to_string()
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not parse remote folder listing: {error}"))
}

fn run_rsync(args: &[String]) -> Result<String, String> {
    let output = Command::new("rsync")
        .args(args)
        .output()
        .map_err(|error| format!("Could not start rsync: {error}"))?;
    output_message(output, "rsync completed successfully.")
}

fn output_message(output: Output, success_message: &str) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(if detail.is_empty() {
            format!("Process exited with status {}.", output.status)
        } else {
            detail.to_string()
        });
    }

    let output = if stdout.trim().is_empty() {
        success_message.to_string()
    } else {
        stdout.trim().to_string()
    };
    Ok(if stderr.trim().is_empty() {
        output
    } else {
        format!("{output}\n{}", stderr.trim())
    })
}

fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ServerProfile {
        ServerProfile {
            id: "profile-id".to_string(),
            name: "Production".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            key_path: Some("/tmp/my key'file".to_string()),
            has_github_token: false,
            git_repositories: Vec::new(),
            git_local_path: None,
            git_remote_path: None,
            git_branch: None,
        }
    }

    fn input(direction: &str, sources: Vec<String>, destination: &str) -> TransferInput {
        TransferInput {
            profile_id: "profile-id".to_string(),
            direction: direction.to_string(),
            sources,
            destination: destination.to_string(),
            excludes: vec!["node_modules/".to_string()],
        }
    }

    #[test]
    fn watch_mode_only_accepts_a_single_local_folder_push() {
        let directory = tempfile::tempdir().expect("create watched directory");
        let file = directory.path().join("file.txt");
        std::fs::write(&file, "content").expect("write source file");

        assert_eq!(
            watch_root_for_input(&input(
                "push",
                vec![directory.path().to_string_lossy().to_string()],
                "/srv/releases",
            ))
            .expect("watch local folder"),
            Some(directory.path().canonicalize().expect("canonical folder"))
        );
        assert_eq!(
            watch_root_for_input(&input(
                "push",
                vec![file.to_string_lossy().to_string()],
                "/srv/releases",
            ))
            .expect("do not watch a single file"),
            None
        );
        assert_eq!(
            watch_root_for_input(&input(
                "pull",
                vec!["/srv/releases".to_string()],
                "/tmp/downloads",
            ))
            .expect("do not watch pulls"),
            None
        );
        assert_eq!(
            watch_root_for_input(&input(
                "push",
                vec![
                    directory.path().to_string_lossy().to_string(),
                    file.to_string_lossy().to_string(),
                ],
                "/srv/releases",
            ))
            .expect("do not watch multiple sources"),
            None
        );
    }

    #[test]
    fn watch_store_allows_only_one_active_session() {
        let store = WatchStore::default();
        let cancelled = Arc::new(AtomicBool::new(false));
        store
            .reserve(
                "first".to_string(),
                "profile-id".to_string(),
                cancelled.clone(),
            )
            .expect("reserve first session");

        assert!(
            store
                .reserve(
                    "second".to_string(),
                    "profile-id".to_string(),
                    Arc::new(AtomicBool::new(false)),
                )
                .is_err()
        );
        store
            .stop_for_profile("another-profile")
            .expect("leave unrelated profile watcher running");
        assert!(!cancelled.load(Ordering::Acquire));
        store
            .stop_for_profile("profile-id")
            .expect("stop the matching profile watcher");
        assert!(cancelled.load(Ordering::Acquire));
        store.clear("first");
        store
            .reserve(
                "second".to_string(),
                "profile-id".to_string(),
                Arc::new(AtomicBool::new(false)),
            )
            .expect("reserve after clearing stopped session");
    }

    #[test]
    fn quotes_ssh_key_path_and_adds_multiple_sources() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second file.txt");
        std::fs::write(&first, "one").expect("write source");
        std::fs::write(&second, "two").expect("write source");
        let (args, source_count) = build_rsync_args(
            &profile(),
            &input(
                "push",
                vec![
                    first.to_string_lossy().to_string(),
                    second.to_string_lossy().to_string(),
                ],
                "/srv/releases",
            ),
            true,
        )
        .expect("build rsync arguments");

        assert_eq!(source_count, 2);
        assert!(args.contains(&"--dry-run".to_string()));
        assert!(args.contains(&"--protect-args".to_string()));
        assert!(args.contains(&"--exclude=node_modules/".to_string()));
        for pattern in PROTECTED_PATHS {
            assert!(args.contains(&format!("--exclude={pattern}").to_string()));
        }
        assert!(
            args.iter()
                .any(|argument| argument == "deploy@example.com:/srv/releases")
        );
        assert!(args.iter().any(|argument| argument.contains("'\"'\"'")));
        assert!(!args.iter().any(|argument| argument == "--delete"));
    }

    #[test]
    fn protected_paths_are_excluded_for_push_and_pull() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let (push_args, _) = build_rsync_args(
            &profile(),
            &input(
                "push",
                vec![directory.path().to_string_lossy().to_string()],
                "/srv/app",
            ),
            true,
        )
        .expect("build push arguments");
        let (pull_args, _) = build_rsync_args(
            &profile(),
            &input(
                "pull",
                vec!["/srv/app".to_string()],
                directory.path().to_str().unwrap(),
            ),
            true,
        )
        .expect("build pull arguments");

        for pattern in PROTECTED_PATHS {
            let argument = format!("--exclude={pattern}");
            assert!(push_args.contains(&argument));
            assert!(pull_args.contains(&argument));
        }
    }

    #[test]
    fn refuses_explicitly_selected_protected_files() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let protected = directory.path().join(".env.production");
        std::fs::write(&protected, "SECRET=value").expect("write fixture");
        assert!(
            build_rsync_args(
                &profile(),
                &input(
                    "push",
                    vec![protected.to_string_lossy().to_string()],
                    "/srv/app"
                ),
                true
            )
            .unwrap_err()
            .contains("Protected files")
        );
        assert!(is_protected_path("/srv/app/config/production.php"));
        assert!(is_protected_path("~/app/server.key"));
        assert!(!is_protected_path("/srv/app/config/development.php"));
    }

    #[test]
    fn rejects_relative_local_paths_and_unsafe_remote_paths() {
        assert!(
            build_rsync_args(
                &profile(),
                &input("push", vec!["relative.txt".to_string()], "/srv/releases"),
                true
            )
            .is_err()
        );
        assert!(
            build_rsync_args(
                &profile(),
                &input("pull", vec!["-eProxyCommand=bad".to_string()], "/tmp"),
                true
            )
            .is_err()
        );
    }

    #[test]
    fn builds_pull_args_for_multiple_remote_sources() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let (args, source_count) = build_rsync_args(
            &profile(),
            &input(
                "pull",
                vec!["/srv/a.txt".to_string(), "~/reports/b.txt".to_string()],
                directory.path().to_str().expect("temp path"),
            ),
            false,
        )
        .expect("build pull arguments");

        assert_eq!(source_count, 2);
        assert!(!args.contains(&"--dry-run".to_string()));
        assert!(
            args.iter()
                .any(|argument| argument == "deploy@example.com:/srv/a.txt")
        );
        assert!(
            args.iter()
                .any(|argument| argument == "deploy@example.com:~/reports/b.txt")
        );
    }

    #[test]
    fn maps_file_names_to_each_transfer_destination() {
        assert_eq!(
            join_remote_file("/srv/releases/", "app.txt").unwrap(),
            "/srv/releases/app.txt"
        );
        assert_eq!(
            join_remote_file("~/releases", "app.txt").unwrap(),
            "~/releases/app.txt"
        );
        assert_eq!(remote_basename("~/reports/app.txt").unwrap(), "app.txt");
        assert!(remote_basename("/srv/reports/").is_err());
    }

    #[test]
    fn text_comparison_rejects_binary_and_non_utf8_files() {
        assert_eq!(decode_text_file(b"first\nsecond").unwrap(), "first\nsecond");
        assert!(decode_text_file(b"text\0binary").is_err());
        assert!(decode_text_file(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn local_comparison_respects_the_file_size_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.txt");
        std::fs::write(&path, vec![b'a'; MAX_COMPARE_FILE_BYTES as usize + 1]).unwrap();

        assert!(read_local_text(&path).unwrap_err().contains("1 MiB"));
    }
}
