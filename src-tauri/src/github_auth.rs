use std::{env, process::Command};

use keyring::Entry;
use tauri::State;

use crate::profiles::ProfileStore;

const KEYRING_SERVICE: &str = "com.nexifyup.rsync-desktop.github";
const ASKPASS_FLAG: &str = "RSYNC_DESKTOP_GITHUB_ASKPASS";
const ASKPASS_PROFILE_ID: &str = "RSYNC_DESKTOP_GITHUB_PROFILE_ID";
const MAX_TOKEN_LENGTH: usize = 512;

fn token_entry(profile_id: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, profile_id)
        .map_err(|error| format!("Could not access the OS credential store: {error}"))
}

pub fn token_for_profile(profile_id: &str) -> Result<Option<String>, String> {
    match token_entry(profile_id)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Could not read the GitHub token from the OS credential store: {error}"
        )),
    }
}

#[tauri::command]
pub fn set_github_token(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
    token: String,
) -> Result<(), String> {
    profiles.get(&profile_id)?;
    let token = validate_token(&token)?;

    token_entry(&profile_id)?
        .set_password(&token)
        .map_err(|error| {
            format!("Could not save the GitHub token to the OS credential store: {error}")
        })
}

fn validate_token(token: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty()
        || token.len() > MAX_TOKEN_LENGTH
        || token.chars().any(char::is_control)
        || token.chars().any(char::is_whitespace)
    {
        return Err("Enter a valid GitHub personal access token.".to_string());
    }
    Ok(token.to_string())
}

#[tauri::command]
pub fn clear_github_token(
    profiles: State<'_, ProfileStore>,
    profile_id: String,
) -> Result<(), String> {
    profiles.get(&profile_id)?;
    clear_token(&profile_id)
}

pub fn clear_token(profile_id: &str) -> Result<(), String> {
    match token_entry(profile_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "Could not remove the GitHub token from the OS credential store: {error}"
        )),
    }
}

pub fn run_askpass() -> Option<i32> {
    if env::var(ASKPASS_FLAG).ok().as_deref() != Some("1") {
        return None;
    }

    let prompt = env::args().nth(1).unwrap_or_default().to_lowercase();
    if !is_github_prompt(&prompt) {
        return Some(1);
    }

    let response = if prompt.contains("username") {
        Ok("x-access-token".to_string())
    } else if is_password_prompt(&prompt) {
        env::var(ASKPASS_PROFILE_ID)
            .map_err(|error| error.to_string())
            .and_then(|profile_id| {
                token_for_profile(&profile_id)?
                    .ok_or_else(|| "No GitHub token is saved for this server profile.".to_string())
            })
    } else {
        Err("GitHub askpass received an unsupported authentication prompt.".to_string())
    };

    match response {
        Ok(response) => {
            println!("{response}");
            Some(0)
        }
        Err(error) => {
            eprintln!("{error}");
            Some(1)
        }
    }
}

pub fn configure_github_token(command: &mut Command, profile_id: &str) -> Result<(), String> {
    let askpass = env::current_exe()
        .map_err(|error| format!("Could not locate the GitHub authentication helper: {error}"))?;
    command
        .env("GIT_ASKPASS", askpass)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(ASKPASS_FLAG, "1")
        .env(ASKPASS_PROFILE_ID, profile_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_entry_is_scoped_to_service_and_profile() {
        assert_eq!(KEYRING_SERVICE, "com.nexifyup.rsync-desktop.github");
    }

    #[test]
    fn token_validation_rejects_empty_and_multiline_values() {
        assert!(validate_token(" ").is_err());
        assert!(validate_token("ghp_valid\ninjected").is_err());
        assert_eq!(
            validate_token(" ghp_valid ").expect("valid token"),
            "ghp_valid"
        );
    }

    #[test]
    fn askpass_uses_token_only_for_password_prompts() {
        assert!(is_password_prompt("Password for 'https://github.com':"));
        assert!(!is_password_prompt("Username for 'https://github.com':"));
        assert!(!is_password_prompt("Approve this action:"));
    }

    #[test]
    fn askpass_only_accepts_github_https_prompts() {
        assert!(is_github_prompt("Password for 'https://github.com':"));
        assert!(is_github_prompt(
            "Password for 'https://x-access-token@github.com':"
        ));
        assert!(!is_github_prompt(
            "Password for 'https://github.com.evil.test':"
        ));
        assert!(!is_github_prompt("Password for 'https://gitlab.com':"));
    }
}

fn is_password_prompt(prompt: &str) -> bool {
    prompt.to_lowercase().contains("password")
}

fn is_github_prompt(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    let Some(scheme) = prompt.split("https://").nth(1) else {
        return false;
    };
    let authority = scheme
        .split(|character: char| matches!(character, '/' | ':' | '\'' | '"' | ' '))
        .next()
        .unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    host == "github.com"
}
