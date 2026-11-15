//! `sandmux db`: conexões de banco no cofre e o registro das queries.

use anyhow::{Result, bail};
use clap::Subcommand;
use sandmux_proto::Msg;
use sandmux_proto::msg::DbConn;
use sandmux_spec::{tr, trf};

use crate::vaultcmd::{Input, call};

#[derive(Subcommand)]
pub enum DbCmd {
    /// Guarda uma conexão no cofre; a senha é lida sem eco (ou da entrada padrão).
    Add {
        nome: String,
        /// `postgres` ou `mysql`.
        #[arg(long)]
        kind: String,
        #[arg(long)]
        host: String,
        /// Padrão: 5432 (postgres) ou 3306 (mysql).
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        database: String,
        #[arg(long)]
        user: String,
    },
    /// Lista as conexões (sem as senhas).
    List,
    /// Apaga uma conexão.
    Remove { nome: String },
    /// Mostra as queries registradas, de uma sessão ou de todas.
    Log {
        id: Option<u32>,
        /// Dias para trás, contando hoje.
        #[arg(long, default_value_t = 1)]
        days: u32,
    },
}

pub fn run(c: DbCmd) -> Result<i32> {
    match c {
        DbCmd::Add {
            nome,
            kind,
            host,
            port,
            database,
            user,
        } => {
            let port = port.unwrap_or(if kind == "mysql" { 3306 } else { 5432 });
            let password = Input::new().secret(&trf!(
                "Senha de {user}@{host}: ",
                "Password for {user}@{host}: "
            ))?;
            call(&Msg::DbAdd {
                conn: DbConn {
                    name: nome.clone(),
                    kind,
                    host,
                    port,
                    database,
                    user,
                    password: password.to_string(),
                },
            })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: conexão {nome} guardada no cofre",
                    "sandmux: connection {nome} saved in the vault"
                )
            );
        }
        DbCmd::List => {
            let Msg::DbEntries { list } = call(&Msg::DbList)? else {
                bail!(tr!(
                    "resposta inesperada do daemon",
                    "unexpected reply from the daemon"
                ));
            };
            if list.is_empty() {
                println!(
                    "{}",
                    tr!(
                        "Nenhuma conexão de banco no cofre.",
                        "No database connections in the vault."
                    )
                );
            }
            for e in list {
                println!(
                    "{}  {}  {}@{}:{}/{}",
                    e.name, e.kind, e.user, e.host, e.port, e.database
                );
            }
        }
        DbCmd::Remove { nome } => {
            call(&Msg::VaultRemove { name: nome.clone() })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: conexão {nome} apagada do cofre",
                    "sandmux: connection {nome} removed from the vault"
                )
            );
        }
        DbCmd::Log { id, days } => return log(id, days),
    }
    Ok(0)
}

/// Uma linha do registro de queries.
#[derive(serde::Deserialize)]
struct Line {
    ts: String,
    tag: String,
    connection: String,
    sql: String,
    rows: Option<u64>,
    ms: u64,
    error: Option<String>,
}

fn log(id: Option<u32>, days: u32) -> Result<i32> {
    let dir = sandmux::paths::state_dir().join("db");
    let session =
        |tag: &str| -> Option<u32> { tag.strip_prefix('s')?.split('-').next()?.parse().ok() };
    let mut n = 0;
    for d in (0..days.max(1)).rev() {
        let f = dir.join(format!("{}.jsonl", sandmux_net::log::day_ago(d)));
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        for l in text
            .lines()
            .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        {
            if id.is_some() && session(&l.tag) != id {
                continue;
            }
            n += 1;
            let hora = l.ts.split('T').nth(1).unwrap_or(&l.ts);
            let s = session(&l.tag).map_or("-".to_string(), |s| s.to_string());
            let r = match (&l.error, l.rows) {
                (Some(e), _) => trf!("erro: {e}", "error: {e}"),
                (None, Some(r)) => trf!("{r} linhas", "{r} rows"),
                _ => String::new(),
            };
            println!(
                "{hora}  {s}  {}  {}ms  {r}  {}",
                l.connection,
                l.ms,
                l.sql.replace('\n', " ")
            );
        }
    }
    if n == 0 {
        println!("{}", tr!("Nenhuma query registrada.", "No queries logged."));
    }
    Ok(0)
}
