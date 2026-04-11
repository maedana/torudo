#[derive(Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    AlreadyLatest(String),
    UpdateAvailable(String),
}

pub fn check_update_needed(current: &str, latest: &str) -> UpdateStatus {
    let current_stripped = current.strip_prefix('v').unwrap_or(current);
    let latest_stripped = latest.strip_prefix('v').unwrap_or(latest);

    if current_stripped == latest_stripped {
        UpdateStatus::AlreadyLatest(latest.to_string())
    } else {
        UpdateStatus::UpdateAvailable(latest.to_string())
    }
}

pub fn perform_update() -> Result<self_update::Status, Box<dyn std::error::Error>> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("maedana")
        .repo_name("torudo")
        .bin_name("torudo")
        .show_download_progress(true)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()?
        .update()?;
    Ok(status)
}

pub fn perform_update_force() -> Result<self_update::Status, Box<dyn std::error::Error>> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("maedana")
        .repo_name("torudo")
        .bin_name("torudo")
        .show_download_progress(true)
        .current_version("0.0.0")
        .build()?
        .update()?;
    Ok(status)
}

pub fn fetch_latest_version() -> Result<String, Box<dyn std::error::Error>> {
    let release = self_update::backends::github::Update::configure()
        .repo_owner("maedana")
        .repo_name("torudo")
        .bin_name("torudo")
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()?
        .get_latest_release()?;
    Ok(release.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_version() {
        assert_eq!(
            check_update_needed("0.8.0", "0.8.0"),
            UpdateStatus::AlreadyLatest("0.8.0".to_string())
        );
    }

    #[test]
    fn test_same_version_with_v_prefix() {
        assert_eq!(
            check_update_needed("0.8.0", "v0.8.0"),
            UpdateStatus::AlreadyLatest("v0.8.0".to_string())
        );
    }

    #[test]
    fn test_update_available() {
        assert_eq!(
            check_update_needed("0.8.0", "0.9.0"),
            UpdateStatus::UpdateAvailable("0.9.0".to_string())
        );
    }

    #[test]
    fn test_update_available_with_v_prefix() {
        assert_eq!(
            check_update_needed("0.8.0", "v0.9.0"),
            UpdateStatus::UpdateAvailable("v0.9.0".to_string())
        );
    }
}
