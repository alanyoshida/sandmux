//! MCP de banco: o socket de um lançamento, o protocolo MCP (JSON-RPC por
//! linhas) e as consultas ao PostgreSQL e ao MySQL com a senha do cofre.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sandmux_proto::msg::DbConn;
use serde_json::{Value, json};

use super::Shared;
use crate::paths;

/// Linhas devolvidas por consulta, no máximo.
pub const MAX_ROWS: usize = 500;

/// O socket de um lançamento; some ao ser solto.
pub struct DbSocket {
    pub path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for DbSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Clone)]
struct Owner {
    tag: String,
    workdir: PathBuf,
    names: Vec<String>,
}

/// Cria o socket do MCP de banco do lançamento `tag`.
pub fn db_socket(
    shared: &Arc<Shared>,
    tag: &str,
    workdir: &std::path::Path,
    names: Vec<String>,
) -> std::io::Result<DbSocket> {
    let dir = paths::runtime_dir().join("db");
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(format!("{tag}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let owner = Owner {
        tag: tag.into(),
        workdir: workdir.into(),
        names,
    };
    let (s2, stop2) = (shared.clone(), stop.clone());
    std::thread::Builder::new()
        .name("db-mcp".into())
        .spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((c, _)) => {
                        let _ = c.set_nonblocking(false);
                        let (s, o) = (s2.clone(), owner.clone());
                        let _ = std::thread::Builder::new()
                            .name("db-mcp-conn".into())
                            .spawn(move || serve(&s, c, &o));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })?;
    Ok(DbSocket { path, stop })
}

fn serve(shared: &Shared, c: UnixStream, owner: &Owner) {
    let Ok(mut w) = c.try_clone() else { return };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => return,
    };
    for line in BufReader::new(c).lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let conns = |names: &[String]| {
            shared
                .vault
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .db_conns(names)
        };
        let unlocked = shared
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unlocked();
        let reply = handle(&line, &owner.names, unlocked, &conns, &|c, call| {
            let t0 = Instant::now();
            let r = rt.block_on(run(c, call));
            log(owner, &c.name, call, &r, t0.elapsed());
            r
        });
        if let Some(r) = reply {
            let mut out = r.to_string();
            out.push('\n');
            if w.write_all(out.as_bytes()).is_err() {
                return;
            }
        }
    }
}

/// Uma chamada a um banco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Tables,
    Describe(String),
    Query(String),
}

impl Call {
    fn sql(&self) -> String {
        match self {
            Call::Tables => "(tabelas)".into(),
            Call::Describe(t) => format!("(descrever {t})"),
            Call::Query(q) => q.clone(),
        }
    }
}

/// Resultado: as linhas (até [`MAX_ROWS`]) e as linhas afetadas.
#[derive(Debug, Default)]
pub struct Outcome {
    pub rows: Vec<serde_json::Map<String, Value>>,
    pub affected: Option<u64>,
    pub truncated: bool,
}

fn tools() -> Value {
    let conn =
        json!({"type": "string", "description": "nome da conexão (veja a ferramenta conexoes)"});
    json!([
        {"name": "conexoes", "description": "Lista as conexões de banco liberadas nesta sessão.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "tabelas", "description": "Lista as tabelas de uma conexão.",
         "inputSchema": {"type": "object", "properties": {"conexao": conn}, "required": ["conexao"]}},
        {"name": "descrever", "description": "Mostra as colunas e os tipos de uma tabela.",
         "inputSchema": {"type": "object", "properties": {"conexao": conn, "tabela": {"type": "string"}}, "required": ["conexao", "tabela"]}},
        {"name": "query", "description": "Executa um SQL (leitura ou escrita) e devolve até 500 linhas ou as linhas afetadas.",
         "inputSchema": {"type": "object", "properties": {"conexao": conn, "sql": {"type": "string"}}, "required": ["conexao", "sql"]}},
    ])
}

fn text(v: Value, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": v.to_string()}], "isError": is_error})
}

/// Atende uma linha JSON-RPC. `None` para notificações.
pub fn handle(
    line: &str,
    names: &[String],
    unlocked: bool,
    conns: &dyn Fn(&[String]) -> Vec<DbConn>,
    exec: &dyn Fn(&DbConn, &Call) -> Result<Outcome, String>,
) -> Option<Value> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(json!({"jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": format!("JSON inválido: {e}")}}));
        }
    };
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));
    let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    match method {
        "initialize" => ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "sandmux-db", "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if !unlocked {
                return ok(text(
                    json!({"erro": "o cofre está trancado; rode `sandmux vault unlock`"}),
                    true,
                ));
            }
            let list = conns(names);
            if name == "conexoes" {
                let v: Vec<Value> = list
                    .iter()
                    .map(|c| json!({"nome": c.name, "tipo": c.kind, "banco": c.database}))
                    .collect();
                return ok(text(Value::Array(v), false));
            }
            let arg = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
            let Some(cname) = arg("conexao") else {
                return ok(text(json!({"erro": "falta `conexao`"}), true));
            };
            let Some(c) = list.iter().find(|c| c.name == cname) else {
                return ok(text(
                    json!({"erro": format!("conexão não liberada nesta sessão: {cname}")}),
                    true,
                ));
            };
            let call = match (name, arg("tabela"), arg("sql")) {
                ("tabelas", _, _) => Call::Tables,
                ("descrever", Some(t), _) => Call::Describe(t),
                ("query", _, Some(q)) => Call::Query(q),
                ("descrever" | "query", _, _) => {
                    return ok(text(json!({"erro": "faltam argumentos"}), true));
                }
                _ => {
                    return Some(json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": -32602, "message": format!("ferramenta desconhecida: {name}")}}));
                }
            };
            match exec(c, &call) {
                Ok(o) => ok(text(
                    json!({"linhas": o.rows, "afetadas": o.affected, "cortado": o.truncated}),
                    false,
                )),
                Err(e) => ok(text(json!({"erro": e}), true)),
            }
        }
        _ => Some(json!({"jsonrpc": "2.0", "id": id,
            "error": {"code": -32601, "message": format!("método desconhecido: {method}")}})),
    }
}

fn quote(s: &str) -> String {
    s.replace('\'', "''")
}

async fn run(c: &DbConn, call: &Call) -> Result<Outcome, String> {
    match c.kind.as_str() {
        "postgres" => postgres(c, call).await,
        "mysql" => mysql(c, call).await,
        k => Err(format!("tipo de banco desconhecido: {k}")),
    }
}

async fn postgres(c: &DbConn, call: &Call) -> Result<Outcome, String> {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host(&c.host)
        .port(c.port)
        .user(&c.user)
        .password(&c.password)
        .dbname(&c.database)
        .connect_timeout(Duration::from_secs(10));
    let (client, conn) = cfg
        .connect(tokio_postgres::NoTls)
        .await
        .map_err(|e| format!("conectar: {e}"))?;
    let task = tokio::spawn(conn);
    let sql = match call {
        Call::Tables => "select table_schema, table_name from information_schema.tables \
             where table_schema not in ('pg_catalog', 'information_schema') order by 1, 2"
            .to_string(),
        Call::Describe(t) => format!(
            "select column_name, data_type, is_nullable from information_schema.columns \
             where table_name = '{}' order by ordinal_position",
            quote(t)
        ),
        Call::Query(q) => q.clone(),
    };
    let msgs = client.simple_query(&sql).await.map_err(|e| e.to_string())?;
    let mut out = Outcome::default();
    for m in msgs {
        match m {
            tokio_postgres::SimpleQueryMessage::Row(r) => {
                if out.rows.len() >= MAX_ROWS {
                    out.truncated = true;
                    continue;
                }
                let mut o = serde_json::Map::new();
                for (i, col) in r.columns().iter().enumerate() {
                    o.insert(
                        col.name().to_string(),
                        r.get(i)
                            .map_or(Value::Null, |v| Value::String(v.to_string())),
                    );
                }
                out.rows.push(o);
            }
            tokio_postgres::SimpleQueryMessage::CommandComplete(n) => out.affected = Some(n),
            _ => {}
        }
    }
    drop(client);
    task.abort();
    Ok(out)
}

fn my_value(v: &mysql_async::Value) -> Value {
    use mysql_async::Value as V;
    match v {
        V::NULL => Value::Null,
        V::Bytes(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
        V::Int(i) => json!(i),
        V::UInt(u) => json!(u),
        V::Float(f) => json!(f),
        V::Double(d) => json!(d),
        other => Value::String(other.as_sql(true).trim_matches('\'').to_string()),
    }
}

async fn mysql(c: &DbConn, call: &Call) -> Result<Outcome, String> {
    use mysql_async::prelude::Queryable;
    let opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(c.host.clone())
        .tcp_port(c.port)
        .user(Some(c.user.clone()))
        .pass(Some(c.password.clone()))
        .db_name(Some(c.database.clone()));
    let mut conn = mysql_async::Conn::new(opts)
        .await
        .map_err(|e| format!("conectar: {e}"))?;
    let sql = match call {
        Call::Tables => "show tables".to_string(),
        Call::Describe(t) => format!("show columns from `{}`", t.replace('`', "``")),
        Call::Query(q) => q.clone(),
    };
    let mut out = Outcome::default();
    {
        let mut r = conn.query_iter(sql).await.map_err(|e| e.to_string())?;
        let rows: Vec<mysql_async::Row> = r.collect().await.map_err(|e| e.to_string())?;
        if rows.is_empty() {
            out.affected = Some(r.affected_rows());
        }
        for row in rows {
            if out.rows.len() >= MAX_ROWS {
                out.truncated = true;
                break;
            }
            let mut o = serde_json::Map::new();
            for (i, col) in row.columns_ref().iter().enumerate() {
                o.insert(
                    col.name_str().into_owned(),
                    row.as_ref(i).map_or(Value::Null, my_value),
                );
            }
            out.rows.push(o);
        }
    }
    let _ = conn.disconnect().await;
    Ok(out)
}

/// Uma linha do registro das queries.
fn log(owner: &Owner, conn: &str, call: &Call, r: &Result<Outcome, String>, took: Duration) {
    let dir = paths::state_dir().join("db");
    let (day, ts) = sandmux_net::log::now_local();
    let line = json!({
        "ts": ts,
        "tag": owner.tag,
        "workdir": owner.workdir,
        "connection": conn,
        "sql": call.sql(),
        "rows": r.as_ref().ok().map(|o| o.affected.unwrap_or(o.rows.len() as u64)),
        "ms": took.as_millis() as u64,
        "error": r.as_ref().err(),
    });
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join(format!("{day}.jsonl")))
    {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str) -> DbConn {
        DbConn {
            name: name.into(),
            kind: "postgres".into(),
            host: "h".into(),
            port: 5432,
            database: "app".into(),
            user: "u".into(),
            password: "segredo".into(),
        }
    }

    #[test]
    fn protocolo_mcp() {
        let names = vec!["app".to_string()];
        let conns = |_: &[String]| vec![conn("app")];
        let exec = |_: &DbConn, call: &Call| -> Result<Outcome, String> {
            match call {
                Call::Query(q) if q == "falha" => Err("erro do banco".into()),
                _ => {
                    let mut o = serde_json::Map::new();
                    o.insert("x".into(), json!("1"));
                    Ok(Outcome {
                        rows: vec![o],
                        affected: None,
                        truncated: false,
                    })
                }
            }
        };
        let call = |l: &str| handle(l, &names, true, &conns, &exec);
        let r = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "sandmux-db");
        assert!(call(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        let r = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 4);
        let r = call(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query","arguments":{"conexao":"app","sql":"select 1 as x"}}}"#).unwrap();
        let t = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(t.contains(r#""x":"1""#), "{t}");
        assert!(!r.to_string().contains("segredo"));
        let r = call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query","arguments":{"conexao":"app","sql":"falha"}}}"#).unwrap();
        assert_eq!(r["result"]["isError"], true);
        let r = call(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query","arguments":{"conexao":"outra","sql":"x"}}}"#).unwrap();
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("não liberada")
        );
        let r = call(r#"{"jsonrpc":"2.0","id":6,"method":"nada"}"#).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"conexoes"}}"#,
            &names,
            false,
            &conns,
            &exec,
        )
        .unwrap();
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("vault unlock")
        );
        let r =
            call(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"conexoes"}}"#)
                .unwrap();
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(r#""nome":"app""#)
        );
    }
}
