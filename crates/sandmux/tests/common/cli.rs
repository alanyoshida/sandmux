//! Execução do `sandmux` com um HOME falso e um diretório de dados temporário.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::{PROBE, SANDMUX, bin_dir};

pub struct Env {
    pub dir: tempfile::TempDir,
    pub home: PathBuf,
    pub data: PathBuf,
    pub work: PathBuf,
}

impl Env {
    /// HOME falso, com uma pasta de projeto dentro dele.
    pub fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let home = root.join("home/ana");
        let data = root.join("data");
        let work = home.join("projetos/app");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        Env {
            dir,
            home,
            data,
            work,
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data.join("sandmux")
    }

    pub fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(SANDMUX);
        c.args(args)
            .current_dir(cwd)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_DATA_HOME", &self.data)
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .stdin(Stdio::null());
        c
    }

    pub fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd(cwd, args).output().unwrap()
    }

    /// `sandmux run shell` com a pasta dos binários de teste visível.
    pub fn shell(&self, extra: &[&str], script: &str) -> Output {
        self.shell_in(&self.work, extra, script)
    }

    pub fn shell_in(&self, cwd: &Path, extra: &[&str], script: &str) -> Output {
        let bins = bin_dir();
        let mut args = vec!["run", "shell", "--ro", bins.to_str().unwrap()];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--", "-c", script]);
        self.run(cwd, &args)
    }

    /// Roda a sonda dentro do sandbox e devolve a última linha JSON.
    pub fn probe(&self, extra: &[&str], probe_args: &[&str]) -> serde_json::Value {
        let script = std::iter::once(PROBE.to_string())
            .chain(probe_args.iter().map(|a| shell_quote(a)))
            .collect::<Vec<_>>()
            .join(" ");
        let out = self.shell(extra, &script);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout.lines().last().unwrap_or_else(|| {
            panic!(
                "sem saída (código {:?}): {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        serde_json::from_str(line).unwrap_or_else(|e| panic!("{line:?}: {e}"))
    }
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
