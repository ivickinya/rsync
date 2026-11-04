use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::State;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepository {
    pub local_path: String,
    pub remote_path: String,
    pub branch: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
    #[serde(default)]
    pub has_github_token: bool,
    #[serde(default)]
    pub git_repositories: Vec<GitRepository>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_remote_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
    #[serde(default)]
    pub git_repositories: Vec<GitRepository>,
}

pub struct ProfileStore {
    path: PathBuf,
    profiles: Mutex<Vec<ServerProfile>>,
}

impl ProfileStore {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let mut profiles: Vec<ServerProfile> = if path.exists() {
            let bytes = fs::read(&path)
                .map_err(|error| format!("Could not read server profiles: {error}"))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| format!("Could not parse saved server profiles: {error}"))?
        } else {
            Vec::new()
        };
        for profile in &mut profiles {
            if profile.git_repositories.is_empty() {
                if let (Some(local_path), Some(remote_path)) = (
                    profile.git_local_path.take(),
                    profile.git_remote_path.take(),
                ) {
                    profile.git_repositories.push(GitRepository {
                        local_path,
                        remote_path,
                        branch: profile.git_branch.take().unwrap_or_else(default_git_branch),
                    });
                }
            }
        }

        Ok(Self {
            path,
            profiles: Mutex::new(profiles),
        })
    }

    fn persist(&self, profiles: &[ServerProfile]) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "Could not determine the profile storage directory.".to_string())?;
        let mut temporary = NamedTempFile::new_in(parent)
            .map_err(|error| format!("Could not create a temporary profile file: {error}"))?;

        serde_json::to_writer_pretty(&mut temporary, profiles)
            .map_err(|error| format!("Could not serialize server profiles: {error}"))?;
        temporary
            .flush()
            .map_err(|error| format!("Could not write server profiles: {error}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| format!("Could not save server profiles: {error}"))?;
        temporary
            .persist(&self.path)
            .map_err(|error| format!("Could not save server profiles: {}", error.error))?;

        Ok(())
    }

    fn profiles(&self) -> Result<Vec<ServerProfile>, String> {
        self.profiles
            .lock()
            .map(|profiles| profiles.clone())
            .map_err(|_| "Server profile storage is unavailable.".to_string())
    }

    pub fn get(&self, profile_id: &str) -> Result<ServerProfile, String> {
        self.profiles
            .lock()
            .map_err(|_| "Server profile storage is unavailable.".to_string())?
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "The selected server profile no longer exists.".to_string())
    }

    fn save(&self, input: ProfileInput) -> Result<ServerProfile, String> {
        let is_new = input.id.is_none();
        let profile = validate_input(input)?;
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "Server profile storage is unavailable.".to_string())?;

        if profiles.iter().any(|existing| {
            existing.name.eq_ignore_ascii_case(&profile.name) && existing.id != profile.id
        }) {
            return Err("A profile with this name already exists.".to_string());
        }

        let mut updated_profiles = profiles.clone();
        let updated_index = updated_profiles
            .iter()
            .position(|existing| existing.id == profile.id);
        if let Some(index) = updated_index {
            updated_profiles[index] = profile.clone();
        } else {
            if !is_new {
                return Err("The selected server profile no longer exists.".to_string());
            }
            updated_profiles.push(profile.clone());
        }

        updated_profiles.sort_by_key(|item| item.name.to_lowercase());
        self.persist(&updated_profiles)?;
        *profiles = updated_profiles;
        Ok(profile)
    }

    fn delete(&self, profile_id: &str) -> Result<(), String> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "Server profile storage is unavailable.".to_string())?;
        let mut updated_profiles = profiles.clone();
        let original_len = updated_profiles.len();
        updated_profiles.retain(|profile| profile.id != profile_id);
        if updated_profiles.len() == original_len {
            return Err("The selected server profile no longer exists.".to_string());
        }
        self.persist(&updated_profiles)?;
        *profiles = updated_profiles;
        Ok(())
    }
}

fn validate_input(input: ProfileInput) -> Result<ServerProfile, String> {
    let name = input.name.trim().to_string();
    let host = input.host.trim().to_string();
    let username = input.username.trim().to_string();
    let key_path = input
        .key_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    let git_repositories = input
        .git_repositories
        .into_iter()
        .enumerate()
        .map(|(index, repository)| validate_git_repository(repository, index + 1))
        .collect::<Result<Vec<_>, _>>()?;

    if name.is_empty() || name.len() > 80 {
        return Err("Profile name must be between 1 and 80 characters.".to_string());
    }
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('-')
        || !host.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '-' | '_' | ':' | '[' | ']')
        })
    {
        return Err("Enter a valid server host or SSH config alias.".to_string());
    }
    if username.is_empty()
        || username.len() > 128
        || username.starts_with('-')
        || !username.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err("Enter a valid SSH username.".to_string());
    }
    if input.port == 0 {
        return Err("SSH port must be between 1 and 65535.".to_string());
    }
    if let Some(path) = &key_path {
        if path.len() > 4096 || path.chars().any(char::is_control) {
            return Err("Enter a valid private key file path.".to_string());
        }
    }
    Ok(ServerProfile {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name,
        host,
        port: input.port,
        username,
        key_path,
        has_github_token: false,
        git_repositories,
        git_local_path: None,
        git_remote_path: None,
        git_branch: None,
    })
}

fn validate_git_repository(
    repository: GitRepository,
    index: usize,
) -> Result<GitRepository, String> {
    let local_path = clean_optional_path(Some(repository.local_path), "local Git repository")?
        .ok_or_else(|| format!("Repository {index}: select a local Git repository."))?;
    let remote_path = clean_optional_path(Some(repository.remote_path), "remote Git checkout")?
        .ok_or_else(|| format!("Repository {index}: enter the server Git checkout path."))?;
    let branch = repository.branch.trim().to_string();

    if !remote_path.starts_with('/') {
        return Err(format!(
            "Repository {index}: the server checkout path must be absolute."
        ));
    }
    if !valid_git_branch(&branch) {
        return Err(format!(
            "Repository {index}: enter a valid Git branch name."
        ));
    }
    Ok(GitRepository {
        local_path,
        remote_path,
        branch,
    })
}

fn default_git_branch() -> String {
    "main".to_string()
}

fn clean_optional_path(path: Option<String>, label: &str) -> Result<Option<String>, String> {
    let path = path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    if path
        .as_ref()
        .is_some_and(|path| path.len() > 4096 || path.contains('\0'))
    {
        return Err(format!("Enter a valid {label} path."));
    }
    Ok(path)
}

fn valid_git_branch(branch: &str) -> bool {
    if branch.is_empty()
        || branch.len() > 255
        || branch == "@"
        || branch.starts_with('-')
        || branch.ends_with(['/', '.'])
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains("@{")
        || branch.chars().any(|character| {
            character.is_ascii_control()
                || matches!(character, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
    {
        return false;
    }

    branch.split('/').all(|component| {
        !component.is_empty() && !component.starts_with('.') && !component.ends_with(".lock")
    })
}

#[tauri::command]
pub fn get_profiles(store: State<'_, ProfileStore>) -> Result<Vec<ServerProfile>, String> {
    let mut profiles = store.profiles()?;
    for profile in &mut profiles {
        profile.has_github_token = crate::github_auth::token_for_profile(&profile.id)?.is_some();
    }
    Ok(profiles)
}

#[tauri::command]
pub fn save_profile(
    store: State<'_, ProfileStore>,
    watchers: State<'_, crate::transfers::WatchStore>,
    profile: ProfileInput,
) -> Result<ServerProfile, String> {
    let updated_id = profile.id.clone();
    let saved = store.save(profile)?;
    if let Some(profile_id) = updated_id {
        watchers.stop_for_profile(&profile_id).map_err(|error| {
            format!(
                "Profile saved, but its active Watch & Sync session could not be stopped: {error}"
            )
        })?;
    }
    Ok(saved)
}

#[tauri::command]
pub fn delete_profile(
    store: State<'_, ProfileStore>,
    watchers: State<'_, crate::transfers::WatchStore>,
    profile_id: String,
) -> Result<(), String> {
    store.get(&profile_id)?;
    crate::github_auth::clear_token(&profile_id)?;
    store.delete(&profile_id)?;
    watchers.stop_for_profile(&profile_id).map_err(|error| {
        format!(
            "Profile deleted, but its active Watch & Sync session could not be stopped: {error}"
        )
    })
}

pub fn storage_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("profiles.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> ProfileInput {
        ProfileInput {
            id: None,
            name: name.to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            key_path: None,
            git_repositories: Vec::new(),
        }
    }

    #[test]
    fn saves_and_reloads_profiles() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let store = ProfileStore::load(storage_path(directory.path())).expect("load store");
        let saved = store.save(input("Production")).expect("save profile");
        assert_eq!(
            store.profiles().expect("list profiles"),
            vec![saved.clone()]
        );

        let reloaded =
            ProfileStore::load(storage_path(directory.path())).expect("reload profile store");
        assert_eq!(reloaded.profiles().expect("list profiles"), vec![saved]);
    }

    #[test]
    fn updates_and_deletes_profiles() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let store = ProfileStore::load(storage_path(directory.path())).expect("load store");
        let saved = store.save(input("Production")).expect("save profile");
        let updated = store
            .save(ProfileInput {
                id: Some(saved.id.clone()),
                name: "Production US".to_string(),
                ..input("ignored")
            })
            .expect("update profile");
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.name, "Production US");

        store.delete(&updated.id).expect("delete profile");
        assert!(store.profiles().expect("list profiles").is_empty());
    }

    #[test]
    fn rejects_invalid_and_duplicate_profiles() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let store = ProfileStore::load(storage_path(directory.path())).expect("load store");
        store.save(input("Production")).expect("save profile");

        assert!(store.save(input("production")).is_err());
        assert!(
            store
                .save(ProfileInput {
                    port: 0,
                    ..input("Staging")
                })
                .is_err()
        );
        assert!(
            store
                .save(ProfileInput {
                    git_repositories: vec![GitRepository {
                        local_path: "/local/repo".to_string(),
                        remote_path: "/srv/repo".to_string(),
                        branch: "main; touch /tmp/pwned".to_string(),
                    }],
                    ..input("Staging")
                })
                .is_err()
        );
        assert!(
            store
                .save(ProfileInput {
                    host: "-oProxyCommand=bad".to_string(),
                    ..input("Staging")
                })
                .is_err()
        );
    }

    #[test]
    fn validates_multiple_git_deployment_settings() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let store = ProfileStore::load(storage_path(directory.path())).expect("load store");
        let saved = store
            .save(ProfileInput {
                git_repositories: vec![
                    GitRepository {
                        local_path: "/local/repo".to_string(),
                        remote_path: "/srv/app".to_string(),
                        branch: "main".to_string(),
                    },
                    GitRepository {
                        local_path: "/local/docs".to_string(),
                        remote_path: "/srv/docs".to_string(),
                        branch: "release".to_string(),
                    },
                ],
                ..input("Production")
            })
            .expect("save Git deployment profile");

        assert_eq!(saved.git_repositories.len(), 2);
        assert_eq!(saved.git_repositories[0].branch, "main");
        assert_eq!(saved.git_repositories[1].branch, "release");
    }

    #[test]
    fn migrates_existing_single_repository_profile() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = storage_path(directory.path());
        fs::write(
            &path,
            r#"[{"id":"id","name":"Production","host":"example.com","port":22,"username":"deploy","keyPath":null,"gitLocalPath":"/local/repo","gitRemotePath":"/srv/app","gitBranch":"main"}]"#,
        )
        .expect("write previous profile format");

        let store = ProfileStore::load(path).expect("load previous profile format");
        let saved = store.profiles().expect("read migrated profile");
        assert_eq!(saved[0].git_repositories.len(), 1);
        assert_eq!(saved[0].git_repositories[0].local_path, "/local/repo");
    }
}
