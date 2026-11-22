//! A seção "Processos" da página de configurações: a árvore de processos da
//! aba ativa, recebida do daemon a cada segundo, com a raiz atual (a migalha
//! de pão), a seleção, o zoom de densidade e as dobras.

use std::collections::{HashMap, HashSet};

use sandmux_proto::msg::ProcNode;

/// Um ramo com mais descendentes que isto nasce dobrado.
pub const AUTO_FOLD: usize = 12;
/// Níveis de zoom: nome e pid; + memória; cartão.
pub const ZOOMS: u8 = 3;

#[derive(Default)]
pub struct ProcsView {
    pub nodes: Vec<ProcNode>,
    /// Sem árvore: o motivo dado pelo daemon.
    pub unavailable: Option<String>,
    /// A árvore já chegou uma vez.
    pub loaded: bool,
    /// A migalha: os pids do caminho até a raiz atual (vazia: a aba).
    pub path: Vec<u32>,
    pub sel: Option<u32>,
    /// De 0 a `ZOOMS - 1`.
    pub zoom: u8,
    /// A versão do auxiliar root em execução, quando é anterior à deste
    /// sandmux: a árvore pode sair sem os pais.
    pub old_helper: Option<String>,
    folded: HashSet<u32>,
    unfolded: HashSet<u32>,
}

/// Uma linha da árvore na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub pid: u32,
    pub depth: usize,
    pub fold: Fold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Leaf,
    Open,
    /// Dobrado, com quantos descendentes escondidos.
    Closed(usize),
}

impl ProcsView {
    pub fn node(&self, pid: u32) -> Option<&ProcNode> {
        self.nodes.iter().find(|n| n.pid == pid)
    }

    /// Filhos de cada pid (0: as raízes), na ordem dos pids.
    fn children(&self) -> HashMap<u32, Vec<u32>> {
        let mut m: HashMap<u32, Vec<u32>> = HashMap::new();
        for n in &self.nodes {
            m.entry(n.ppid).or_default().push(n.pid);
        }
        for v in m.values_mut() {
            v.sort_unstable();
        }
        m
    }

    fn descendants(ch: &HashMap<u32, Vec<u32>>, pid: u32, seen: &mut HashSet<u32>) -> usize {
        let mut n = 0;
        for &c in ch.get(&pid).into_iter().flatten() {
            if seen.insert(c) {
                n += 1 + Self::descendants(ch, c, seen);
            }
        }
        n
    }

    fn folded_with(&self, ch: &HashMap<u32, Vec<u32>>, pid: u32) -> bool {
        if self.folded.contains(&pid) {
            return true;
        }
        !self.unfolded.contains(&pid)
            && Self::descendants(ch, pid, &mut HashSet::from([pid])) > AUTO_FOLD
    }

    /// As raízes da tela: o ramo em que se entrou, ou as raízes da aba.
    fn starts(&self, ch: &HashMap<u32, Vec<u32>>) -> Vec<u32> {
        match self.path.last() {
            Some(&r) => vec![r],
            None => ch.get(&0).cloned().unwrap_or_default(),
        }
    }

    /// As linhas visíveis a partir da raiz atual. As raízes da tela ficam
    /// sempre abertas.
    pub fn rows(&self) -> Vec<Row> {
        let ch = self.children();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let starts = self.starts(&ch);
        for &s in &starts {
            self.walk(&ch, s, 0, &starts, &mut seen, &mut out);
        }
        out
    }

    fn walk(
        &self,
        ch: &HashMap<u32, Vec<u32>>,
        pid: u32,
        depth: usize,
        starts: &[u32],
        seen: &mut HashSet<u32>,
        out: &mut Vec<Row>,
    ) {
        if !seen.insert(pid) {
            return;
        }
        let kids = ch.get(&pid).map(Vec::as_slice).unwrap_or(&[]);
        let fold = if kids.is_empty() {
            Fold::Leaf
        } else if !starts.contains(&pid) && self.folded_with(ch, pid) {
            Fold::Closed(Self::descendants(ch, pid, &mut HashSet::from([pid])))
        } else {
            Fold::Open
        };
        out.push(Row { pid, depth, fold });
        if fold == Fold::Open {
            for &k in kids {
                self.walk(ch, k, depth + 1, starts, seen, out);
            }
        }
    }

    /// Uma árvore nova do daemon: a raiz atual que terminou volta até um
    /// ascendente na migalha; a seleção fica onde estava, se continua na
    /// tela, e as dobras feitas à mão continuam.
    pub fn set(&mut self, nodes: Vec<ProcNode>, unavailable: Option<String>) {
        self.nodes = nodes;
        self.unavailable = unavailable;
        self.loaded = true;
        while let Some(&p) = self.path.last() {
            if self.node(p).is_some() {
                break;
            }
            self.path.pop();
        }
        let live: HashSet<u32> = self.nodes.iter().map(|n| n.pid).collect();
        self.folded.retain(|p| live.contains(p));
        self.unfolded.retain(|p| live.contains(p));
        self.keep_sel();
    }

    fn keep_sel(&mut self) {
        let rows = self.rows();
        if !self.sel.is_some_and(|p| rows.iter().any(|r| r.pid == p)) {
            self.sel = rows.first().map(|r| r.pid);
        }
    }

    /// A posição da seleção nas linhas.
    pub fn sel_index(&self, rows: &[Row]) -> Option<usize> {
        let p = self.sel?;
        rows.iter().position(|r| r.pid == p)
    }

    /// Move a seleção `delta` linhas.
    pub fn step(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let i = self.sel_index(&rows).unwrap_or(0) as isize;
        let j = (i + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.sel = Some(rows[j].pid);
    }

    fn sel_row(&self) -> Option<Row> {
        let p = self.sel?;
        self.rows().into_iter().find(|r| r.pid == p)
    }

    /// Dobra a seleção aberta; `false` quando não havia o que dobrar (as
    /// raízes da tela não dobram).
    pub fn fold(&mut self) -> bool {
        let starts = self.starts(&self.children());
        match self.sel_row() {
            Some(r) if r.fold == Fold::Open && !starts.contains(&r.pid) => {
                self.folded.insert(r.pid);
                self.unfolded.remove(&r.pid);
                true
            }
            _ => false,
        }
    }

    /// Desdobra a seleção dobrada; `false` quando ela não estava dobrada.
    pub fn unfold(&mut self) -> bool {
        match self.sel_row() {
            Some(r) if matches!(r.fold, Fold::Closed(_)) => {
                self.folded.remove(&r.pid);
                self.unfolded.insert(r.pid);
                true
            }
            _ => false,
        }
    }

    pub fn toggle(&mut self) {
        if !self.unfold() {
            self.fold();
        }
    }

    /// Entra no ramo da seleção: ela vira a raiz da tela e o fim da migalha.
    pub fn enter(&mut self) {
        if let Some(p) = self.sel
            && self.path.last() != Some(&p)
            && self.node(p).is_some()
        {
            self.path.push(p);
        }
    }

    /// Volta um nível na migalha; `false` já na raiz. A seleção fica no
    /// ramo de onde se saiu.
    pub fn back(&mut self) -> bool {
        let Some(p) = self.path.pop() else {
            return false;
        };
        self.sel = Some(p);
        self.keep_sel();
        true
    }

    /// Volta ao segmento `i` da migalha (0: a aba).
    pub fn crumb(&mut self, i: usize) {
        if i < self.path.len() {
            let from = self.path[i];
            self.path.truncate(i);
            self.sel = Some(from);
            self.keep_sel();
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom + 1).min(ZOOMS - 1);
    }

    pub fn zoom_out(&mut self) {
        self.zoom = self.zoom.saturating_sub(1);
    }
}

/// Bytes em K, M ou G, curto.
pub fn size(b: u64) -> String {
    match b {
        b if b >= 1 << 30 => format!("{:.1}G", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{}M", b >> 20),
        b => format!("{}K", b >> 10),
    }
}

/// Uma duração em segundos, curta (`4min`, `2h`, `3d`).
pub fn age(s: u64) -> String {
    match s {
        s if s >= 86400 => format!("{}d", s / 86400),
        s if s >= 3600 => format!("{}h", s / 3600),
        s if s >= 60 => format!("{}min", s / 60),
        s => format!("{s}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(pid: u32, ppid: u32) -> ProcNode {
        ProcNode {
            pid,
            ppid,
            name: format!("p{pid}"),
            cmdline: String::new(),
            rss: 0,
            cpu_ms: 0,
            age_secs: 0,
            cgroup: String::new(),
        }
    }

    /// bash(10) → node(11) → npm(12); bash → sh(13).
    fn view() -> ProcsView {
        let mut v = ProcsView::default();
        v.set(vec![n(10, 0), n(11, 10), n(12, 11), n(13, 10)], None);
        v
    }

    fn pids(v: &ProcsView) -> Vec<(u32, usize)> {
        v.rows().iter().map(|r| (r.pid, r.depth)).collect()
    }

    #[test]
    fn arvore_inteira_em_outline() {
        let v = view();
        assert_eq!(pids(&v), [(10, 0), (11, 1), (12, 2), (13, 1)]);
        assert_eq!(v.sel, Some(10));
    }

    #[test]
    fn entrar_dois_niveis_e_voltar_um_de_cada_vez() {
        let mut v = view();
        v.step(1);
        v.enter();
        assert_eq!(v.path, [11]);
        assert_eq!(pids(&v), [(11, 0), (12, 1)]);
        v.step(1);
        v.enter();
        assert_eq!(v.path, [11, 12]);
        assert_eq!(pids(&v), [(12, 0)]);
        assert!(v.back());
        assert_eq!((v.path.clone(), v.sel), (vec![11], Some(12)));
        assert!(v.back());
        assert_eq!((v.path.clone(), v.sel), (vec![], Some(11)));
        assert!(!v.back());
        // O clique num segmento anterior da migalha.
        v.step(1);
        v.enter();
        v.enter();
        assert_eq!(v.path, [12]);
        v.crumb(0);
        assert!(v.path.is_empty());
    }

    #[test]
    fn dobrar_e_desdobrar() {
        let mut v = view();
        v.step(1);
        assert!(v.fold());
        assert_eq!(pids(&v), [(10, 0), (11, 1), (13, 1)]);
        assert_eq!(v.rows()[1].fold, Fold::Closed(1));
        assert!(!v.fold());
        v.toggle();
        assert_eq!(pids(&v), [(10, 0), (11, 1), (12, 2), (13, 1)]);
        // A dobra feita à mão continua depois de uma árvore nova.
        v.fold();
        v.set(
            vec![n(10, 0), n(11, 10), n(12, 11), n(13, 10), n(14, 10)],
            None,
        );
        assert_eq!(v.rows()[1].fold, Fold::Closed(1));
        assert_eq!(v.sel, Some(11));
    }

    #[test]
    fn raiz_da_aba_fica_aberta() {
        let mut nodes = vec![n(10, 0)];
        nodes.extend((100..100 + AUTO_FOLD as u32 + 1).map(|p| n(p, 10)));
        let mut v = ProcsView::default();
        v.set(nodes, None);
        assert_eq!(v.rows().len(), AUTO_FOLD + 2);
        assert!(!v.fold());
    }

    #[test]
    fn ramo_grande_nasce_dobrado() {
        let mut nodes = vec![n(10, 0), n(11, 10)];
        nodes.extend((100..100 + AUTO_FOLD as u32 + 1).map(|p| n(p, 11)));
        let mut v = ProcsView::default();
        v.set(nodes, None);
        assert_eq!(v.rows()[1].fold, Fold::Closed(AUTO_FOLD + 1));
        assert_eq!(v.rows().len(), 2);
        // Ao entrar no ramo, ele é a raiz e fica aberto.
        v.step(1);
        v.enter();
        assert_eq!(v.rows().len(), AUTO_FOLD + 2);
        assert_eq!(v.rows()[0].fold, Fold::Open);
        v.back();
        assert!(v.unfold());
        assert_eq!(v.rows().len(), AUTO_FOLD + 3);
    }

    #[test]
    fn atualizacao_preserva_selecao_e_volta_quando_a_raiz_termina() {
        let mut v = view();
        v.step(1);
        v.enter();
        v.step(1);
        assert_eq!(v.sel, Some(12));
        v.zoom_in();
        v.set(vec![n(10, 0), n(11, 10), n(12, 11), n(15, 11)], None);
        assert_eq!((v.path.clone(), v.sel, v.zoom), (vec![11], Some(12), 1));
        // O 11 terminou: os filhos foram para o 10, e a tela volta à aba.
        v.set(vec![n(10, 0), n(12, 10), n(15, 10)], None);
        assert!(v.path.is_empty());
        assert_eq!(v.sel, Some(12));
        // O selecionado terminou: a seleção vai para a primeira linha.
        v.set(vec![n(10, 0)], None);
        assert_eq!(v.sel, Some(10));
        v.set(Vec::new(), Some("supervisor: sem o auxiliar".into()));
        assert_eq!(v.sel, None);
        assert!(v.rows().is_empty());
    }

    #[test]
    fn zoom_nos_limites() {
        let mut v = view();
        for _ in 0..5 {
            v.zoom_in();
        }
        assert_eq!(v.zoom, ZOOMS - 1);
        for _ in 0..5 {
            v.zoom_out();
        }
        assert_eq!(v.zoom, 0);
        assert_eq!(size(4096), "4K");
        assert_eq!(size(412 << 20), "412M");
        assert_eq!(age(250), "4min");
    }
}
