//! Texto em português fora de `tr!`/`trf!`: só os listados em
//! `pt_only.txt` (valores gravados ou comparados, texto para o agente).

mod common;

use common::cli::*;
use std::collections::BTreeSet;

fn literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '"' {
            let mut s = String::new();
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    s.push(b[i]);
                    i += 1;
                }
                s.push(b[i]);
                i += 1;
            }
            // Só os textos que fecham na mesma linha.
            if i < b.len() {
                out.push(s);
            }
        }
        i += 1;
    }
    out
}

fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn texto_em_portugues_passa_por_tr() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let allowed: BTreeSet<String> =
        std::fs::read_to_string(root.join("crates/sandmux/tests/pt_only.txt"))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(str::to_string)
            .collect();
    let accents = "ãõçáéíóúâêôàÃÕÇÁÉÍÓÚ";
    let mut files = Vec::new();
    for c in std::fs::read_dir(root.join("crates")).unwrap() {
        let src = c.unwrap().path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut missing = Vec::new();
    for f in files {
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().to_string();
        if rel.contains("sandmux-probe") || rel.contains("sandmux-db-mcp") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let end = lines
            .iter()
            .position(|l| l.trim() == "#[cfg(test)]")
            .unwrap_or(lines.len());
        for i in 0..end {
            let l = lines[i];
            if l.trim_start().starts_with("//") {
                continue;
            }
            for v in literals(l) {
                if !v.chars().any(|c| accents.contains(c)) {
                    continue;
                }
                let ctx = lines[i.saturating_sub(3)..=i].join(" ");
                if ctx.contains("tr!(") || ctx.contains("trf!(") {
                    continue;
                }
                if ["expect(", "panic!", "assert", "unreachable", "debug_assert"]
                    .iter()
                    .any(|k| l.contains(k))
                {
                    continue;
                }
                // Par bilíngue: o português seguido do inglês (`"pt", "en"`),
                // em tabelas ou argumentos que escolhem com `tr!`.
                let next = lines.get(i + 1).map(|n| n.trim_start()).unwrap_or("");
                if l.contains(&format!("\"{v}\", \""))
                    || (l.trim_end().ends_with(&format!("\"{v}\",")) && next.starts_with('"'))
                {
                    continue;
                }
                let key = format!("{rel}\t{v}");
                if !allowed.contains(&key) {
                    missing.push(format!("{rel}:{}: {v}", i + 1));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "texto em português fora de tr!/trf! (traduza, ou acrescente a pt_only.txt se for um valor gravado ou comparado):\n{}",
        missing.join("\n")
    );
}

#[test]
fn linha_de_comando_no_idioma_escolhido() {
    let e = Env::new();
    let set = |lang: Option<(&str, &str)>| {
        let mut c = e.cmd(&e.work, &["config", "set", "kvm", "talvez", "--global"]);
        if let Some((k, v)) = lang {
            c.env(k, v);
        }
        let out = c.output().unwrap();
        assert!(!out.status.success());
        stderr(&out)
    };
    // SANDMUX_LANG passa por cima de tudo; os nomes das chaves não mudam.
    let s = set(Some(("SANDMUX_LANG", "en")));
    assert!(
        s.contains("sandmux: error:") && s.contains("invalid value for `kvm`"),
        "{s}"
    );
    // O sistema em inglês, sem nada na configuração.
    let s = set(Some(("LANG", "en_US.UTF-8")));
    assert!(s.contains("invalid value for `kvm`"), "{s}");
    // A configuração passa por cima do sistema.
    e.write_config("[global]\nlanguage = \"pt\"\n");
    let s = set(Some(("LANG", "en_US.UTF-8")));
    assert!(
        s.contains("sandmux: erro:") && s.contains("valor inválido para `kvm`"),
        "{s}"
    );
    // `yes`/`no` também valem.
    let out = e
        .cmd(&e.work, &["config", "set", "kvm", "yes", "--global"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    // Idioma inválido torna o arquivo inválido.
    e.write_config("[global]\nlanguage = \"fr\"\n");
    let out = e.run(&e.work, &["config", "show"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("`language`"), "{}", stderr(&out));
    // A ajuda em inglês.
    e.write_config("");
    let out = e
        .cmd(&e.work, &["--help"])
        .env("SANDMUX_LANG", "en")
        .output()
        .unwrap();
    let help = stdout(&out);
    assert!(help.contains("Lists the sessions"), "{help}");
}

#[test]
fn daemon_responde_no_idioma_do_cliente() {
    use common::pty::Term;
    let e = Env::new();
    // A sessão e o daemon sobem em português.
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut s = Term::spawn(cmd, 100, 30);
    s.write(b"echo pronto-$((1+1))\r");
    s.wait_text("pronto-2");
    s.write(b"\x02d");
    s.wait_exit();
    // Um cliente em inglês recebe o erro do daemon em inglês, e o resumo
    // gravado em português aparece em inglês.
    let en = |args: &[&str]| {
        let out = e
            .cmd(&e.work, args)
            .env("SANDMUX_LANG", "en")
            .output()
            .unwrap();
        format!("{}{}", stdout(&out), stderr(&out))
    };
    let out = en(&["db", "list"]);
    assert!(out.contains("there is no vault"), "{out}");
    let out = en(&["stats", "1"]);
    assert!(
        out.contains("network: open, logged") && out.contains("level 1"),
        "{out}"
    );
    let out = en(&["config", "show"]);
    assert!(out.contains("open (aberta)"), "{out}");
    // E o português continua em português.
    let out = e.run(&e.work, &["db", "list"]);
    assert!(stderr(&out).contains("não há cofre"), "{}", stderr(&out));
}
