#![allow(dead_code)]

use std::path::{Path, PathBuf};

use inker_agents::Env;
use serde_json::Value;
use tempfile::TempDir;

pub struct TestHome {
    pub dir: TempDir,
    pub env: Env,
}

impl TestHome {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let env = Env::for_home(
            &home,
            Path::new("/opt/inker/bin/inker"),
            &home.join("Private"),
        );
        Self { dir, env }
    }

    pub fn home(&self) -> &Path {
        &self.env.home
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.env.home.join(rel)
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap()
    }

    pub fn json(&self, rel: &str) -> Value {
        serde_json::from_str(&self.read(rel)).unwrap()
    }

    /// A fake executable on a private PATH that appends its argv to `<name>.log`.
    pub fn fake_bin(&mut self, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let bin_dir = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let p = bin_dir.join(name);
        let log = self.dir.path().join(format!("{name}.log"));
        std::fs::write(
            &p,
            format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        self.env.path_var = Some(bin_dir.into_os_string());
        p
    }

    pub fn fake_log(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(format!("{name}.log"))).unwrap_or_default()
    }
}
