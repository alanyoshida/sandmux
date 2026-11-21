//! O `bwrap` do sandmux dentro do sandbox: executa sem montar, com o
//! Landlock empilhado pelas montagens pedidas.

mod common;
use common::cli::*;

fn run(e: &Env, script: &str) -> (String, String) {
    let out = e.shell(&[], script);
    (stdout(&out), stderr(&out))
}

#[test]
fn bwrap_no_path_e_binds_viram_landlock() {
    let e = Env::new();
    let script = r#"
command -v bwrap
bwrap --ro-bind / / --bind "$PWD" "$PWD" -- sh -c 'touch "$PWD/x" && echo X-OK; touch "$HOME/y" 2>/dev/null && echo Y-GRAVOU || echo Y-NEGADO'
"#;
    let (out, err) = run(&e, script);
    assert!(out.contains("/run/sandmux/bin/bwrap"), "{out}\n{err}");
    assert!(
        out.contains("X-OK") && out.contains("Y-NEGADO"),
        "{out}\n{err}"
    );
    assert!(e.work.join("x").exists());
}

#[test]
fn bwrap_namespaces_sem_montagem_e_status() {
    let e = Env::new();
    let script = r#"
bwrap --unshare-net --unshare-pid --unshare-uts --hostname caixa --die-with-parent --new-session \
  --proc /proc --dev /dev --dev-bind / / --info-fd 3 -- sh -c 'echo pid=$$; hostname; ls /sys/class/net' 3>info.json
cat info.json
bwrap --clearenv --setenv A 1 --chdir / -- /bin/sh -c 'echo "A=$A HOME=${HOME:-vazio} PWD=$PWD"'
bwrap --nao-existe -- true; echo rc=$?
"#;
    let (out, err) = run(&e, script);
    assert!(out.contains("pid=1"), "{out}\n{err}");
    assert!(out.contains("caixa"), "{out}\n{err}");
    // Só o `lo` no namespace de rede novo.
    assert!(out.lines().any(|l| l == "lo"), "{out}\n{err}");
    assert!(out.contains("\"child-pid\""), "{out}\n{err}");
    assert!(out.contains("A=1 HOME=vazio PWD=/"), "{out}\n{err}");
    assert!(out.contains("rc=1"), "{out}\n{err}");
}
