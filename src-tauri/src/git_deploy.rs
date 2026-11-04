use std::process::{Command, Output};

use tauri::State;

use crate::github_auth;
use crate::profiles::{GitRepository, ProfileStore, ServerProfile};

#[tauri::command]
pub async fn deploy_git_repository(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
    repository_index: usize,
) -> Result<String, String> {
    let profile = profiles.get(&profile_id)?;
    let repository = profile
        .git_repositories
        .get(repository_index)
        .cloned()
        .ok_or_else(|| "The selected Git repository is no longer configured.".to_string())?;
    let has_token = github_auth::token_for_profile(&profile.id)?.is_some();
    tauri::async_runtime::spawn_blocking(move || deploy(&profile, &repository, has_token))
        .await
        .map_err(|error| format!("Could not deploy Git branch: {error}"))?
}

fn deploy(
    profile: &ServerProfile,
    repository: &GitRepository,
    has_token: bool,
) -> Result<String, String> {
    let local_root = run_local_git(["-C", &repository.local_path, "rev-parse", "--show-toplevel"])?;
    let local_root = local_root.trim();
    if local_root.is_empty() {
        return Err("Could not determine the local Git repository root.".to_string());
    }
    let local_branch = run_local_git(["-C", local_root, "branch", "--show-current"])?;
    if local_branch.trim() != repository.branch {
        return Err(format!(
            "The local repository is on branch '{}', not '{}'. Switch to the configured branch before deploying.",
            local_branch.trim(),
            repository.branch
        ));
    }
    let local_origin = run_local_git(["-C", local_root, "remote", "get-url", "origin"])?;
    if has_token && !is_github_https_origin(local_origin.trim()) {
        return Err(
            "A GitHub token is configured, but this repository's origin is not an HTTPS GitHub URL. Use https://github.com/OWNER/REPOSITORY.git or remove the token."
                .to_string(),
        );
    }

    let remote_status = run_remote_git(
        profile,
        &repository.remote_path,
        "status --porcelain --untracked-files=all",
    )?;
    if !remote_status.trim().is_empty() {
        return Err(
            "The server Git checkout has local changes. Commit, stash, or remove them before deploying."
                .to_string(),
        );
    }

    let remote_branch = run_remote_git(profile, &repository.remote_path, "branch --show-current")?;
    if remote_branch.trim() != repository.branch {
        return Err(format!(
            "The server checkout is on branch '{}', not '{}'. Switch the checkout to the configured branch before deploying.",
            remote_branch.trim(),
            repository.branch
        ));
    }
    let remote_origin = run_remote_git(profile, &repository.remote_path, "remote get-url origin")?;
    if local_origin.trim() != remote_origin.trim() {
        return Err(
            "The local repository and server checkout have different origin URLs. Configure both checkouts to use the same Git repository before deploying."
                .to_string(),
        );
    }

    let mut push_command = Command::new("git");
    push_command
        .arg("-C")
        .arg(local_root)
        .args(["push", "origin"])
        .arg(format!("HEAD:refs/heads/{}", repository.branch))
        .env("GIT_TERMINAL_PROMPT", "0");
    if has_token {
        github_auth::configure_github_token(&mut push_command, &profile.id)?;
    }
    let push = push_command
        .output()
        .map_err(|error| format!("Could not start Git push: {error}"))?;
    let push_output = successful_output(push, "Git push failed")?;

    let pull_output = run_remote_git(
        profile,
        &repository.remote_path,
        &format!("pull --ff-only origin {}", shell_quote(&repository.branch)),
    )
    .map_err(|error| {
        format!("Git push succeeded, but updating the server checkout failed: {error}")
    })?;

    Ok(format!(
        "Pushed committed changes to origin/{} and fast-forwarded the server checkout.\n{}{}",
        repository.branch,
        push_output.trim(),
        if pull_output.trim().is_empty() {
            String::new()
        } else {
            format!("\n{}", pull_output.trim())
        }
    ))
}

fn run_local_git<const N: usize>(args: [&str; N]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("Could not start Git: {error}"))?;
    successful_output(output, "Local Git command failed")
}

fn run_remote_git(
    profile: &ServerProfile,
    repository_path: &str,
    git_arguments: &str,
) -> Result<String, String> {
    let remote_command = format!(
        "GIT_TERMINAL_PROMPT=0 git -C {} {}",
        shell_quote(repository_path),
        git_arguments
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
    let output = command
        .arg(format!("{}@{}", profile.username, profile.host))
        .arg(remote_command)
        .output()
        .map_err(|error| format!("Could not start SSH: {error}"))?;
    successful_output(output, "Remote Git command failed")
}

fn successful_output(output: Output, action: &str) -> Result<String, String> {
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if detail.is_empty() {
        Err(format!("{action} with status {}.", output.status))
    } else {
        Err(format!("{action}: {detail}"))
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_github_https_origin(origin: &str) -> bool {
    let Some(authority_and_path) = origin.strip_prefix("https://") else {
        return false;
    };
    let authority = authority_and_path.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = authority.strip_suffix(":443").unwrap_or(authority);
    host.eq_ignore_ascii_case("github.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_remote_paths_as_single_arguments() {
        assert_eq!(shell_quote("/srv/it's safe"), "'/srv/it'\\''s safe'");
    }

    #[test]
    fn token_only_applies_to_https_github_origins() {
        assert!(is_github_https_origin(
            "https://github.com/owner/repository.git"
        ));
        assert!(!is_github_https_origin(
            "https://user@github.com/owner/repository.git"
        ));
        assert!(!is_github_https_origin(
            "https://github.example.com/owner/repository.git"
        ));
        assert!(!is_github_https_origin(
            "git@github.com:owner/repository.git"
        ));
    }
}
