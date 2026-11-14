//! O mínimo de DNS que o gateway precisa: ler o nome consultado, responder o
//! nome da máquina e ler os registros A de uma resposta.

use std::net::Ipv4Addr;

/// Uma consulta: identificador, nome (em minúsculas, sem o ponto final) e
/// tipo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub id: u16,
    pub name: String,
    pub qtype: u16,
    /// Fim da seção de perguntas, para montar a resposta.
    pub end: usize,
}

/// Lê um nome a partir de `pos`, seguindo ponteiros de compressão. Devolve o
/// nome e a posição depois dele (no fluxo original).
fn read_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    let mut jumps = 0;
    loop {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xc0 == 0xc0 {
            let ptr = ((len & 0x3f) << 8) | *msg.get(pos + 1)? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            jumps += 1;
            if jumps > 16 {
                return None;
            }
            pos = ptr;
            continue;
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).to_ascii_lowercase());
        pos += 1 + len;
        if labels.len() > 128 {
            return None;
        }
    }
    Some((labels.join("."), end.unwrap_or(pos)))
}

/// Lê a primeira pergunta de uma consulta.
pub fn parse_query(msg: &[u8]) -> Option<Query> {
    if msg.len() < 12 {
        return None;
    }
    let id = u16::from_be_bytes([msg[0], msg[1]]);
    let qd = u16::from_be_bytes([msg[4], msg[5]]);
    if qd == 0 {
        return None;
    }
    let (name, p) = read_name(msg, 12)?;
    let qtype = u16::from_be_bytes([*msg.get(p)?, *msg.get(p + 1)?]);
    Some(Query {
        id,
        name,
        qtype,
        end: p + 4,
    })
}

/// Resposta a uma consulta com um registro A (ou vazia, para outros tipos).
pub fn answer_a(query: &[u8], q: &Query, ip: Ipv4Addr) -> Vec<u8> {
    let mut out = query[..q.end.min(query.len())].to_vec();
    // Cabeçalho: resposta, autoritativa, recursão disponível, sem erro.
    out[2] = 0x84 | (query[2] & 0x01);
    out[3] = 0x80;
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    let an: u16 = if q.qtype == 1 { 1 } else { 0 };
    out[6..8].copy_from_slice(&an.to_be_bytes());
    out[8..12].copy_from_slice(&[0, 0, 0, 0]);
    if an == 1 {
        out.extend_from_slice(&[0xc0, 0x0c]); // ponteiro para o nome
        out.extend_from_slice(&1u16.to_be_bytes()); // A
        out.extend_from_slice(&1u16.to_be_bytes()); // IN
        out.extend_from_slice(&60u32.to_be_bytes()); // TTL
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&ip.octets());
    }
    out
}

/// Resposta NXDOMAIN (nome inexistente) a uma consulta.
pub fn nxdomain(query: &[u8], q: &Query) -> Vec<u8> {
    let mut out = query[..q.end.min(query.len())].to_vec();
    // Resposta, opcode e RD copiados, recursão disponível, código 3.
    out[2] = 0x80 | (query[2] & 0x79);
    out[3] = 0x80 | 3;
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    out[6..12].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
    out
}

/// Registros A de uma resposta: (nome consultado, IP).
pub fn parse_a_answers(msg: &[u8]) -> Vec<(String, Ipv4Addr)> {
    let mut out = Vec::new();
    let Some(q) = parse_query(msg) else {
        return out;
    };
    let an = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    let mut p = q.end;
    for _ in 0..an.min(64) {
        let Some((_, after)) = read_name(msg, p) else {
            break;
        };
        let Some(h) = msg.get(after..after + 10) else {
            break;
        };
        let rtype = u16::from_be_bytes([h[0], h[1]]);
        let rdlen = u16::from_be_bytes([h[8], h[9]]) as usize;
        let data = after + 10;
        if rtype == 1
            && rdlen == 4
            && let Some(b) = msg.get(data..data + 4)
        {
            out.push((q.name.clone(), Ipv4Addr::new(b[0], b[1], b[2], b[3])));
        }
        p = data + rdlen;
    }
    out
}

#[cfg(test)]
pub fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut m = id.to_be_bytes().to_vec();
    m.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for l in name.split('.') {
        m.push(l.len() as u8);
        m.extend_from_slice(l.as_bytes());
    }
    m.push(0);
    m.extend_from_slice(&qtype.to_be_bytes());
    m.extend_from_slice(&1u16.to_be_bytes());
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consulta_e_resposta() {
        let q = build_query(0x1234, "Host.Sandmux.Internal", 1);
        let p = parse_query(&q).unwrap();
        assert_eq!(
            (p.id, p.name.as_str(), p.qtype),
            (0x1234, "host.sandmux.internal", 1)
        );
        let r = answer_a(&q, &p, Ipv4Addr::new(10, 0, 2, 254));
        assert_eq!(&r[0..2], &[0x12, 0x34]);
        assert_eq!(
            parse_a_answers(&r),
            vec![("host.sandmux.internal".into(), Ipv4Addr::new(10, 0, 2, 254))]
        );
        // AAAA: resposta sem registros.
        let q6 = build_query(7, "host.sandmux.internal", 28);
        let p6 = parse_query(&q6).unwrap();
        assert!(parse_a_answers(&answer_a(&q6, &p6, Ipv4Addr::LOCALHOST)).is_empty());
        assert!(parse_query(&[0; 5]).is_none());
        let nx = nxdomain(&q, &p);
        assert_eq!(&nx[0..2], &[0x12, 0x34]);
        assert_eq!(nx[3] & 0x0f, 3);
        assert_eq!(nx[2] & 0x80, 0x80);
        assert!(parse_a_answers(&nx).is_empty());
        assert_eq!(parse_query(&nx).unwrap().name, "host.sandmux.internal");
    }
}
