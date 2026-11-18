//! Locations of everything BitNot stores under the per-user AppData folder.
//! Original music files never live here and are never touched by this module.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn new(root: PathBuf) -> Result<Self> {
        let paths = AppPaths { root };
        fs::create_dir_all(&paths.root)?;
        fs::create_dir_all(paths.logs_dir())?;
        fs::create_dir_all(paths.profiles_dir())?;
        fs::create_dir_all(paths.artwork_dir())?;
        Ok(paths)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn app_db(&self) -> PathBuf {
        self.root.join("app.db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn session_marker(&self) -> PathBuf {
        self.root.join("session.lock")
    }

    /// Directory holding one profile's internal data. The id is validated so a
    /// malformed id can never point outside the profiles folder.
    pub fn profile_dir(&self, profile_id: &str) -> Result<PathBuf> {
        let valid = !profile_id.is_empty()
            && profile_id.len() <= 64
            && profile_id
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-');
        if !valid {
            return Err(AppError::invalid("Invalid profile identifier."));
        }
        Ok(self.profiles_dir().join(profile_id))
    }

    /// Cached cover images. Files are named by content hash, so the cache is
    /// shared by all profiles and holds each cover once.
    pub fn artwork_dir(&self) -> PathBuf {
        self.root.join("artwork")
    }

    pub fn profile_db(&self, profile_id: &str) -> Result<PathBuf> {
        Ok(self.profile_dir(profile_id)?.join("profile.db"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data")).unwrap();
        assert!(paths.logs_dir().is_dir());
        assert!(paths.profiles_dir().is_dir());
    }

    #[test]
    fn rejects_traversal_in_profile_id() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().to_path_buf()).unwrap();
        assert!(paths.profile_dir("..").is_err());
        assert!(paths.profile_dir("a/b").is_err());
        assert!(paths.profile_dir("..\\x").is_err());
        assert!(paths.profile_dir("").is_err());
        assert!(paths
            .profile_dir("3f2b8c1e-0000-4000-8000-123456789abc")
            .is_ok());
    }
}
