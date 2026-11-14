//! Tetos de memória e de processos por sandbox.

mod common;
use common::cli::*;

/// Os tetos só funcionam com o systemd de usuário; sem ele, o teste pula.
fn supported(e: &Env) -> bool {
    let out = e.shell(&[], "true");
    let ok = out.status.success() && !stderr(&out).contains("sem teto de memória");
    if !ok {
        eprintln!(
            "pulado: sem suporte a tetos de recurso (código {:?}): {}",
            out.status.code(),
            stderr(&out)
        );
    }
    ok
}

const CG: &str = "cg=/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)";

#[test]
fn teto_de_processos_recusa_fork_dentro_do_sandbox() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    let script = format!(
        "{CG}; cat $cg/pids.max; sh -c 'for i in $(seq 1 100); do sleep 3 & done' 2>&1 | grep -c -i fork; echo depois"
    );
    let out = e.shell(&["--processes", "64"], &script);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    let mut l = s.lines();
    assert_eq!(l.next(), Some("64"), "{s}");
    let fails: u32 = l.next().unwrap().trim().parse().unwrap();
    assert!(fails > 0, "nenhum fork recusado: {s}");
    assert_eq!(l.next(), Some("depois"));
}

#[test]
fn estouro_de_memoria_mata_so_o_programa() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    e.write_config(&format!(
        "[project.\"{}\"]\nmemory = \"512M\"\n",
        e.work.display()
    ));
    let script = format!(
        "{CG}; cat $cg/memory.max $cg/memory.swap.max; python3 -c 'a = bytearray(2 << 30)'; echo python=$?; grep oom_kill $cg/memory.events; echo shell-vivo"
    );
    let out = e.shell(&[], &script);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(s.starts_with(&format!("{}\n0\n", 512u64 << 20)), "{s}");
    assert!(s.contains("python=137"), "{s}");
    assert!(s.contains("oom_kill 1"), "{s}");
    assert!(s.ends_with("shell-vivo\n"), "{s}");
}

#[test]
fn sem_teto_de_memoria_mantem_o_de_processos() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    let out = e.shell(
        &["--memory", "max"],
        &format!("{CG}; cat $cg/memory.max $cg/pids.max"),
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "max\n8192\n");
}

#[test]
fn tetos_padrao_batem_com_a_ram() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    let total = std::fs::read_to_string("/proc/meminfo")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 1024;
    let out = e.shell(&[], &format!("{CG}; cat $cg/memory.max $cg/pids.max"));
    let s = stdout(&out);
    let mut l = s.lines();
    let mem: u64 = l.next().unwrap().parse().unwrap();
    // O systemd arredonda para a página.
    let want = total / 100 * 75;
    assert!(mem.abs_diff(want) < 8192, "{mem} != {want}");
    assert_eq!(l.next(), Some("8192"));
}

#[test]
fn valores_invalidos_recusados() {
    let e = Env::new();
    let out = e.shell(&["--memory", "64M"], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("128M"), "{}", stderr(&out));
    let out = e.shell(&["--processes", "abc"], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("--processes"), "{}", stderr(&out));
    let out = e.run(&e.work, &["run", "--help"]);
    let h = stdout(&out);
    assert!(h.contains("--memory") && h.contains("--processes"), "{h}");
}

#[test]
fn sem_suporte_sobe_sem_teto_e_avisa() {
    let e = Env::new();
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--ro",
                common::bin_dir().to_str().unwrap(),
                "--",
                "-c",
                "echo subiu",
            ],
        )
        // Sem o systemd-run no PATH.
        .env("PATH", e.dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "subiu\n");
    let err = stderr(&out);
    assert!(
        err.contains("sem teto de memória e de processos") && err.contains("sandmux doctor"),
        "{err}"
    );
}

#[test]
fn config_set_e_show_dos_tetos() {
    let e = Env::new();
    let out = e.run(&e.work, &["config", "set", "memory", "4G", "--project"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = e.run(&e.work, &["config", "set", "processes", "16", "--global"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("32"), "{}", stderr(&out));
    let s = stdout(&e.run(&e.work, &["config", "show"]));
    let row = |k: &str| {
        s.lines()
            .find(|l| l.split_whitespace().next() == Some(k))
            .unwrap_or_else(|| panic!("sem {k}: {s}"))
            .to_string()
    };
    assert!(
        row("memory").contains("4G") && row("memory").contains("modificada (projeto"),
        "{s}"
    );
    assert!(
        row("processes").contains("8192") && row("processes").ends_with("padrão"),
        "{s}"
    );
}

// ---- sessões: ls, stats e memória esgotada ----

use common::pty::Term;

fn term(e: &Env, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, 120, 30)
}

fn ls(e: &Env) -> String {
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn line_of(list: &str, id: u32) -> String {
    list.lines()
        .find(|l| l.starts_with(&format!("{id} ")))
        .unwrap_or_else(|| panic!("sem a sessão {id}:\n{list}"))
        .to_string()
}

#[test]
fn ls_e_stats_mostram_o_consumo() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    let mut t = term(
        &e,
        &["run", "shell", "--processes", "64", "--memory", "256M"],
    );
    t.write(b"echo pronto-$((2+2))\r");
    t.wait_text("pronto-4");
    // Uma morte por falta de memória e recusas de fork, com a sessão viva.
    t.write(b"python3 -c 'a = bytearray(1 << 30)'; echo py=$?\r");
    t.wait_text("py=137");
    t.write(b"sh -c 'for i in $(seq 1 100); do sleep 2 & done' 2>/dev/null; echo forks-$((1+1))\r");
    t.wait_text("forks-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // Encerrada, para a coluna vazia.
    let mut t2 = term(&e, &["run", "shell"]);
    t2.write(b"exit 3\r");
    assert_eq!(t2.wait_exit(), 3);

    let list = ls(&e);
    let head = list.lines().next().unwrap();
    assert!(head.contains("MEMÓRIA") && head.contains("PROC"), "{list}");
    let l1 = line_of(&list, 1);
    assert!(l1.contains("/256M") && l1.contains("/64"), "{list}");
    let l2 = line_of(&list, 2);
    assert!(
        l2.contains("encerrada (3)") && !l2.contains("/8192"),
        "{list}"
    );

    let out = e.run(&e.work, &["stats", "1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(
        s.contains("teto de 256M") && s.contains("teto de 64"),
        "{s}"
    );
    assert!(s.contains("mortes por falta de memória: 1"), "{s}");
    assert!(!s.contains("recusadas pelo teto: 0"), "{s}");
    assert!(s.contains("pico de 25"), "pico perto do teto: {s}");

    let out = e.run(&e.work, &["stats", "9"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("não existe a sessão 9"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn harness_morto_pelo_teto_aparece_como_memoria_esgotada() {
    let e = Env::new();
    if !supported(&e) {
        return;
    }
    let mut t = term(
        &e,
        &[
            "run",
            "shell",
            "--memory",
            "256M",
            "--",
            "-c",
            "python3 -c 'a = bytearray(1 << 30)'",
        ],
    );
    assert_eq!(t.wait_exit(), 137);
    let list = ls(&e);
    assert!(
        line_of(&list, 1).contains("encerrada (137, memória esgotada)"),
        "{list}"
    );
    let s = stdout(&e.run(&e.work, &["stats", "1"]));
    assert!(
        s.contains("encerrada (137, memória esgotada)")
            && s.contains("mortes por falta de memória: 1"),
        "{s}"
    );
}

#[test]
fn doctor_mostra_os_tetos() {
    let e = Env::new();
    let out = e.run(&e.work, &["doctor"]);
    let s = stdout(&out);
    if supported(&e) {
        assert!(
            s.contains("[ok  ] tetos de memória e de processos") && s.contains("8192 processos"),
            "{s}"
        );
    }
    // Sem o systemd-run no PATH: a falha aparece e o código não muda por isso.
    let out = e
        .cmd(&e.work, &["doctor"])
        .env("PATH", e.dir.path())
        .output()
        .unwrap();
    let s2 = stdout(&out);
    assert!(
        s2.contains("tetos de memória e de processos")
            && s2.contains("systemd-run não está no PATH"),
        "{s2}"
    );
    assert_eq!(out.status.code(), e.run(&e.work, &["doctor"]).status.code());
}
