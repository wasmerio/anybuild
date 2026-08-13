//! Mount and volume path layout.

use std::path::{Path, PathBuf};

/// Where mounts and volumes live for a given backend/runner pair.
///
/// The evaluation host asks this for build and serve paths when the
/// Anybuild file calls `mount()` / `volume()`.
pub trait MountLayout {
    fn build_mount_path(&self, name: &str) -> PathBuf;
    fn serve_mount_path(&self, name: &str) -> PathBuf;
    fn volume_path(&self, name: &str) -> PathBuf;
}

/// The local backend layout: `anybuild_dir/local/build/{app,opt/<name>}`,
/// volumes under `anybuild_dir/volumes`. The local runner serves from the
/// build artifacts, so serve paths equal build paths.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct LocalLayout {
    pub anybuild_dir: PathBuf,
}

#[cfg(test)]
impl LocalLayout {
    pub fn new(anybuild_dir: impl Into<PathBuf>) -> Self {
        Self {
            anybuild_dir: anybuild_dir.into(),
        }
    }

    pub fn build_path(&self) -> PathBuf {
        self.anybuild_dir.join("local").join("build")
    }

    fn mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            self.build_path().join("app")
        } else {
            self.build_path().join("opt").join(name)
        }
    }
}

#[cfg(test)]
impl MountLayout for LocalLayout {
    fn build_mount_path(&self, name: &str) -> PathBuf {
        self.mount_path(name)
    }

    fn serve_mount_path(&self, name: &str) -> PathBuf {
        self.mount_path(name)
    }

    fn volume_path(&self, name: &str) -> PathBuf {
        self.anybuild_dir.join("volumes").join(name)
    }
}

/// Container serve-side layout: `/app` for the app mount, `/opt/<name>`
/// for everything else.
#[derive(Debug, Clone)]
pub struct ContainerServeLayout;

impl ContainerServeLayout {
    pub fn serve_mount_path(name: &str) -> PathBuf {
        if name == "app" {
            Path::new("/app").to_path_buf()
        } else {
            Path::new("/opt").join(name)
        }
    }
}
