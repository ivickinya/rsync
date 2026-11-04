# Rsync Desktop (Tauri + Rust + Web)

A modern, fast, and lightweight cross-platform GUI for **rsync over SSH**, built with **Tauri 2 (Rust core)** and a **Twig / HTML / custom CSS / Vanilla JavaScript** frontend.

## Current Status

The project contains a Tauri dashboard, SSH connection management, and a push/pull transfer workflow. Server profiles can be added, edited, and deleted; they are stored as JSON in the app's local data directory. A transfer can select multiple local files/folders or multiple newline-separated remote paths. Local sources and remote destinations can be browsed over the filesystem and SSH respectively. Transfer previews also support comparing individual text files with their remote/local counterpart in side-by-side or unified view. SSH testing, dry-run preview, and a separately confirmed live rsync transfer are implemented. Private-key contents are never imported or stored.

Watch & Sync is available for one local folder at a time. It starts only from a reviewed dry-run preview, performs the approved initial sync, then watches recursively and debounces changes for 500 ms before running an incremental rsync. It stays active until stopped or the app closes. Transfer history, streamed progress updates, and cancelling an in-progress rsync are not implemented yet.

Twig is rendered to static HTML before Tauri starts or builds the app. The frontend is plain HTML, CSS, and JavaScript; Node.js is not required.

---

## 🚀 Product Features

- 🎨 **Automatic Appearance**
  - Follows the operating system's light or dark appearance; there is no in-app theme switch.

- 🖥️ **Server Profiles & Credential Manager**
  - Save multiple remote servers (e.g., `Project Production`, `Project Staging`, `Backup Server`).
  - Configure Host, SSH Port (default 22), Username, and SSH Private Key paths (`~/.ssh/id_rsa`, `~/.ssh/id_ed25519`).
- 🚀 **Git Deployment**
  - Add multiple local repositories to one server profile, each with its own absolute server checkout path and branch (defaults to `main`).
  - Deploy pushes each repository's committed `HEAD` to `origin/<branch>`, then runs a fast-forward-only pull in its server checkout over SSH.
  - An optional GitHub Personal Access Token (classic; `repo` scope for private repositories) is stored in the OS credential store and used only for local pushes to HTTPS `github.com` origins. It is never written to profile JSON, command arguments, or sent to the server.
  - Each checkout must be clean and already on the configured branch, and both repositories must use the same `origin`. Uncommitted local changes are not pushed. The server needs its own GitHub authentication for `git pull`; use an SSH deploy key or server-side Git credential.
- 🖥️ **macOS Developer UX**
  - Server profiles can launch an SSH session in Terminal.app.
  - Completed and failed rsync transfers request native desktop notifications; delivery follows the OS notification permission.
  
- 🧪 **Safety-First Dry Run Mode (`--dry-run`)**
  - Preview exactly what files will be added, modified, or deleted **before** executing live syncs.
  - Every live transfer requires a successful preview followed by explicit confirmation.
  - Protected environment, production configuration, and common key/certificate file patterns are excluded from both push and pull transfers; explicitly selecting a protected file is rejected. Destination-only files are not deleted.
  - The preview lists planned additions and updates in color-coded rows. Automatic backup snapshots and rollback are still planned and are not implemented yet.

- 🧰 **Multiple Sources & Exclusions**
  - Browse local folders, navigate subfolders, and select individual files or folders for push, or enter multiple remote paths for pull.
  - Optional presets exclude `.git/`, `node_modules/`, `vendor/`, `.env`, `*.pem`, `*.key`, and `.DS_Store`.
- 📝 **File Comparison**
  - During transfer preview, compare an individual selected file with the same filename at the transfer destination, using side-by-side or unified diff.
  - Text comparison supports UTF-8 files up to 1 MiB each; folders and binary files are not rendered as text.
- 👁️ **Live Watch & Sync**
  - After previewing a single local folder and confirming the live sync, automatically push folder changes to the selected server with a 500 ms debounce.
  - Shows watching, syncing, and error states; stop the watcher from the transfer view. Editing or deleting its connection stops the watcher after any current rsync finishes. The watcher is local to the current app session and is stopped when the app closes.
  - Protected-file exclusions remain active and remote-only files are never deleted. The feature is for local-to-remote push only.

- ⚡ **Transfer Progress** (planned)
  - Stream progress, speed, elapsed time, and ETA while a transfer runs.

- 🛡️ **Smart Exclusion Presets**
  - Built-in toggles for standard ignorable folders:
    - Version control: `.git/`, `.svn/`, `.hg/`
    - Dependencies: `node_modules/`, `vendor/`
    - Environment & Secrets: `.env`, `.env.local`, `*.pem`, `*.key`
    - Caches & Temporary: `storage/cache/`, `var/cache/`, `tmp/`, `.DS_Store`
  - Custom `.rsyncignore` and custom exclude/include patterns.

- 🔄 **Bidirectional & Backup Capabilities**
  - Push: Local ➔ Remote
  - Pull: Remote ➔ Local
  - Optional `--backup` with `--backup-dir` to preserve replaced files.

---

## 🔮 Advanced Workflow Specifications & Roadmap

### 1. 🛡️ Safety Guardrails & Disaster Prevention
- **Current:** Protected paths are excluded in both directions, explicit protected-file selections are rejected, and previews show classified additions/updates. Rsync deletion is not enabled.
- **Protected Files Shield (Never-Overwrite Rule):**
  - Hard-lock critical files (e.g., `.env`, `.env.production`, `config/production.php`, SSL certificates, and JWT keys).
  - Even during full folder syncs, local development configurations can **never overwrite** server-side production credentials.
- **Visual Pre-Flight Tree Diff:**
  - Interactive, color-coded file tree preview during `--dry-run`:
    - 🟢 **Green (+):** New files to upload
    - 🟡 **Yellow (~):** Existing files to update
    - 🔴 **Red (-):** Files marked for deletion (when `--delete` is enabled)
- **One-Click Instant Rollback (`--backup-dir`):**
  - Rsync automatically archives replaced or deleted remote files into a timestamped snapshot directory (e.g., `~/.rsync_backups/2026-11-04_23-15/`).
  - An instant **"Rollback"** action restores the previous snapshot over SSH if a deployment encounters issues.

### 2. ⚡ Pre-Sync & Post-Sync Automation Hooks (SSH Triggers)
**Status: Planned.** Hooks are not run by the current application.
Execute remote and local shell commands automatically before and after transfers:
- **Pre-Sync Hooks:**
  - Check remote disk space (`df -h /`) before dispatching large transfers.
  - Enable maintenance mode (`php console down` or touch maintenance flag).
- **Post-Sync Hooks (Executed automatically over SSH):**
  - **Permission Enforcement:** Automatically run `chown -R www-data:www-data` and `chmod -R 775 storage/` on target paths.
  - **Framework & Cache Clearing:** Run `php console cache:clear`, `php artisan optimize:clear`, or OPcache reset.
  - **Service Management:** Reload web servers or process managers (`sudo systemctl reload apache2`, `sudo systemctl restart php8.2-fpm`).
  - **Database Migration Trigger:** Optional trigger to run pending schema migrations post-deploy.

### 3. 🎯 Task & Operational Presets
**Status: Planned.** The current transfer view does not yet include operational preset templates.
Pre-configured sync profiles tailored for common server operations:
1. **Code Deploy:** Synchronizes application code; excludes logs, temporary uploads, Git internals, and environment files.
2. **Pull Production Logs:** 1-click download of remote error logs (e.g., `/var/log/apache2/error.log` or `storage/logs/*.log`) directly into a local viewer for rapid debugging.
3. **Pull Remote Uploads/Media:** Downloads user-uploaded media (e.g., `public/assets/uploads/`, documents, images) to local development environments.
4. **Database Dump Transfer:** Securely transfers nightly MySQL `.sql.gz` dumps from remote backup directories.

### 4. 👁️ Live "Watch & Sync" Mode (Continuous Development)
**Status: Implemented.** Select exactly one local folder, create and review a dry-run preview, then choose **Sync & Watch Folder** and confirm the warning. The app performs that initial sync before enabling continuous watching.
- Uses the operating system's filesystem notification backend to watch recursively and debounces file activity for 500 ms. Rsync runs incrementally against the same folder and destination, transferring changed content without enabling `--delete`.
- The transfer view reports watching, syncing, errors, and stop status. Watchers are not saved across app restarts; stop a session explicitly when finished. Editing or deleting the associated connection stops it after any current rsync finishes.
- Only local-to-remote push is supported. Existing protected-path exclusions remain in effect; remote-only files and files deleted locally are not removed from the server.

### 5. 🖥️ macOS System Integration & Developer UX
- **Current:** Profile cards include an SSH Terminal action; transfer completion/failure requests native notifications.
- **Still planned:** macOS menu bar quick-widget, verified backup/rollback, and richer notification actions/status history. SSH hooks, operational presets, and watch mode are also not implemented yet.
- **Direct "Open SSH Terminal" Action:**
  - 1-click button on each server profile to launch a direct SSH session in your preferred terminal (Terminal.app, iTerm2, or Warp).
- **macOS Menu Bar Quick-Widget:**
  - Lightweight menu bar status icon showing recent sync health, last deployment time, and a quick-trigger menu for favorite sync tasks.
- **Native macOS Notifications:**
  - System banners and sound alerts upon successful sync completion, dry-run warnings, or connection failures.

---

## 🏗️ Architecture

```
rsync/
├── README.md                  # Project documentation & build guide
├── composer.json              # PHP/Twig frontend build dependencies
├── scripts/
│   └── build-frontend.php     # Renders ui/index.twig into the Tauri asset directory
├── src-tauri/                 # Rust Backend (Tauri 2.x)
│   ├── Cargo.toml             # Rust dependencies
│   ├── tauri.conf.json        # Tauri app configuration
│   ├── capabilities/          # Tauri permissions
│   ├── icons/                 # App icons
│   └── src/
│       ├── main.rs            # Application entry point
│       ├── lib.rs             # Tauri application setup and IPC registration
│       ├── filesystem.rs      # Local folder listing for the file browser
│       ├── git_deploy.rs      # Git push and safe remote fast-forward deployment
│       ├── profiles.rs        # Local server profile storage and validation
│       └── transfers.rs       # SSH checks, rsync previews and execution
└── ui/                        # Web Frontend (No Node.js runtime required)
    ├── index.twig             # Main dashboard template
    ├── css/
    │   └── app.css            # Dashboard styling
    └── js/
        └── app.js             # Transfer direction controls
```

---

## 🛠️ Prerequisites

- **Rust & Cargo:** `1.99.1+` (Installed ✅)
- **Tauri CLI:** `tauri-cli 2.12.4+` (`cargo-tauri` Installed ✅)
- **System rsync:** macOS built-in or Homebrew rsync (`rsync --version`)
- **OpenSSH client:** `ssh -V`
- **Git:** installed locally and on the server for Git deployment
- **PHP & Composer:** PHP `8.2+` and Composer (`composer install`)

Server profiles are persisted in the Tauri application data directory (`profiles.json`).
The selected private-key file remains where it is on disk; only its path is saved.
Browsing remote destination folders requires Python 3 to be installed on the SSH server.
For local-to-server transfers, browse a local folder and check individual files or folders,
or add the currently browsed folder itself. You can also use the native file picker to select
multiple files. For server-to-local transfers, enter one absolute remote
source path per line. The selected destination must be a folder.

---

## 💻 Development & Build Commands

Install the Twig renderer once from the project root:
```bash
composer install
```

### 1. Run in Development Mode
Renders the Twig template and launches the Tauri app:
```bash
cd /Applications/XAMPP/xamppfiles/htdocs/rsync
cargo tauri dev
```

### 2. Build Production macOS App Bundle (.app / .dmg)
Renders the Twig template and generates an optimized macOS application:
```bash
cargo tauri build
```
The output `.app` and `.dmg` will be located in:
```
src-tauri/target/release/bundle/macos/
src-tauri/target/release/bundle/dmg/
```

---

## 📋 Rust IPC Commands

| Tauri Command | Arguments | Description |
| :--- | :--- | :--- |
| `list_local_directory` | `(root_path, path)` | Lists files and folders inside the selected local folder |
| `get_profiles` | `()` | Loads all saved server profiles |
| `save_profile` | `(profile: Profile)` | Creates or updates a server profile |
| `delete_profile` | `(profile_id: String)` | Removes a saved profile |
| `test_ssh_connection` | `(profile_id)` | Tests a saved SSH profile |
| `deploy_git_repository` | `(profile_id, repository_index)` | Pushes committed HEAD to origin and fast-forwards the configured clean server checkout |
| `list_remote_directory` | `(profile_id, root_path, path)` | Lists a remote folder over SSH, scoped to the chosen browser root |
| `compare_transfer_file` | `(input)` | Compares one local text file with its transfer counterpart over SSH |
| `preview_rsync` | `(input)` | Runs a dry-run and returns a single-use preview token |
| `execute_rsync` | `(preview_id)` | Runs the previously previewed transfer after confirmation |
| `rollback_snapshot` | `(profile_id, snapshot_path, destination_path)` | Restores a previously created timestamped backup over SSH *(Planned)* |
| `execute_ssh_hook` | `(profile_id, command)` | Executes pre- or post-sync SSH maintenance scripts *(Planned)* |
| `start_directory_watcher` | `(preview_id)` | Runs an approved initial sync, then starts watching its single local folder |
| `stop_directory_watcher` | `(watcher_id)` | Stops the active directory watcher after any current sync finishes |
| `open_ssh_terminal` | `(profile_id)` | Launches user's native macOS terminal into an active SSH session *(Planned)* |

---

## 📄 License
Internal Development Tool — NexifyUp & Playvertize Ecosystem.
