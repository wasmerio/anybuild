//! Mount/volume path layout, mirroring `builders/local.py` + `runners/*.py`.

use std::path::{Path, PathBuf};

/// Where mounts and volumes live for a given backend/runner pair.
///
/// The evaluation host asks this for build and serve paths when the
/// Shipit file calls `mount()` / `volume()`.
pub trait MountLayout {
    fn build_mount_path(&self, name: &str) -> PathBuf;
    fn serve_mount_path(&self, name: &str) -> PathBuf;
    fn volume_path(&self, name: &str) -> PathBuf;
}

/// The local backend layout: `shipit_dir/local/build/{app,opt/<name>}`,
/// volumes under `shipit_dir/volumes`. The local runner serves from the
/// build artifacts, so serve paths equal build paths.
#[derive(Debug, Clone)]
pub struct LocalLayout {
    pub shipit_dir: PathBuf,
}

impl LocalLayout {
    pub fn new(shipit_dir: impl Into<PathBuf>) -> Self {
        Self { shipit_dir: shipit_dir.into() }
    }

    pub fn build_path(&self) -> PathBuf {
        self.shipit_dir.join("local").join("build")
    }

    fn mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            self.build_path().join("app")
        } else {
            self.build_path().join("opt").join(name)
        }
    }
}

impl MountLayout for LocalLayout {
    fn build_mount_path(&self, name: &str) -> PathBuf {
        self.mount_path(name)
    }

    fn serve_mount_path(&self, name: &str) -> PathBuf {
        self.mount_path(name)
    }

    fn volume_path(&self, name: &str) -> PathBuf {
        self.shipit_dir.join("volumes").join(name)
    }
}

/// Wasmer serve-side layout: `/app` for the app mount, `/opt/<name>`
/// for everything else (`runners/wasmer.py: get_serve_mount_path`).
#[derive(Debug, Clone)]
pub struct WasmerServeLayout;

impl WasmerServeLayout {
    pub fn serve_mount_path(name: &str) -> PathBuf {
        if name == "app" {
            Path::new("/app").to_path_buf()
        } else {
            Path::new("/opt").join(name)
        }
    }
}
