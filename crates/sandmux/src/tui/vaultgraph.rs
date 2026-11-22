//! A tela do grafo do cofre: os domínios donos numa lista, o grupo escolhido
//! no centro com os segredos em volta, e a janela de zoom e movimento. Aqui
//! ficam o modelo, as posições e a janela, sem terminal; o desenho está em
//! `render.rs` e as teclas em `settings.rs`.

use std::collections::BTreeMap;

use sandmux_proto::msg::{DbEntry, VaultEntry};

/// Os passos do zoom.
pub const ZOOMS: [f64; 7] = [0.5, 0.64, 0.8, 1.0, 1.25, 1.6, 2.0];
/// O passo de 1.0x.
pub const ZOOM_ONE: usize = 3;
/// A partir desta largura, lista, grafo e detalhes ficam lado a lado.
pub const WIDE_COLS: u16 = 100;
/// Abaixo desta largura, só a lista.
pub const MIN_COLS: u16 = 70;
/// Largura da lista e dos detalhes.
pub const LIST_W: u16 = 26;
pub const DETAIL_W: u16 = 28;

const RING_FIRST: usize = 8;
const RING_STEP: usize = 4;
const PAN_STEP: f64 = 2.0;
/// Quanto o centro da janela pode sair da caixa dos nós.
const PAN_MARGIN: (f64, f64) = (20.0, 10.0);

/// Um item da lista da esquerda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Group {
    Domain(String),
    /// Os segredos sem domínio dono.
    NoDomain,
    /// As conexões de banco.
    Dbs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Token,
    Ssh,
    Db,
}

impl Kind {
    /// A marca diz o tipo sem depender da cor.
    pub fn mark(self) -> char {
        match self {
            Kind::Token => '●',
            Kind::Ssh => '◆',
            Kind::Db => '■',
        }
    }
}

/// Um nó em volta do centro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub kind: Kind,
    /// Quantos outros domínios donos o segredo tem.
    pub others: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub group: Group,
    pub members: Vec<Member>,
}

/// Quanto cada nó mostra, pelo zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Mark,
    Name,
    Card,
}

/// Os grupos da lista: os domínios em ordem alfabética, depois os segredos
/// sem domínio e as conexões de banco, cada grupo só quando tem algum. As
/// entradas de tipo `db` de `VaultList` vêm sem domínio e ficam fora dos
/// dois primeiros.
pub fn build(entries: &[VaultEntry], dbs: &[DbEntry]) -> Vec<Row> {
    let mut by_domain: BTreeMap<&str, Vec<Member>> = BTreeMap::new();
    let mut none = Vec::new();
    for e in entries.iter().filter(|e| e.kind != "db") {
        let kind = if e.kind == "ssh" {
            Kind::Ssh
        } else {
            Kind::Token
        };
        let mut doms: Vec<&str> = e.domains.iter().map(String::as_str).collect();
        doms.sort_unstable();
        doms.dedup();
        if doms.is_empty() {
            none.push(Member {
                name: e.name.clone(),
                kind,
                others: 0,
            });
        }
        for d in &doms {
            by_domain.entry(d).or_default().push(Member {
                name: e.name.clone(),
                kind,
                others: doms.len() - 1,
            });
        }
    }
    let by_name = |m: &mut Vec<Member>| m.sort_by(|a, b| a.name.cmp(&b.name));
    let mut rows: Vec<Row> = by_domain
        .into_iter()
        .map(|(d, mut members)| {
            by_name(&mut members);
            Row {
                group: Group::Domain(d.to_string()),
                members,
            }
        })
        .collect();
    if !none.is_empty() {
        by_name(&mut none);
        rows.push(Row {
            group: Group::NoDomain,
            members: none,
        });
    }
    let mut db: Vec<Member> = dbs
        .iter()
        .map(|d| Member {
            name: d.name.clone(),
            kind: Kind::Db,
            others: 0,
        })
        .collect();
    if !db.is_empty() {
        by_name(&mut db);
        rows.push(Row {
            group: Group::Dbs,
            members: db,
        });
    }
    rows
}

/// Quantos nós o anel `k` leva: 8, 12, 16, cada um 4 a mais que o anterior.
pub fn ring_capacity(k: usize) -> usize {
    RING_FIRST + RING_STEP * k
}

/// Quantos anéis `n` nós ocupam.
#[cfg(test)]
pub fn rings(n: usize) -> usize {
    let (mut left, mut k) = (n, 0);
    while left > 0 {
        left -= left.min(ring_capacity(k));
        k += 1;
    }
    k
}

/// As posições de `n` nós em volta da origem, em ângulos iguais por anel. A
/// unidade é uma linha do terminal; na horizontal, uma unidade vale duas
/// colunas. Nenhum nó do primeiro anel cai na linha do centro, onde fica a
/// caixa do domínio.
pub fn layout(n: usize) -> Vec<(f64, f64)> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let mut out = Vec::with_capacity(n);
    let (mut left, mut k) = (n, 0usize);
    while left > 0 {
        let m = left.min(ring_capacity(k));
        let (rx, ry) = (8.0 + 6.0 * k as f64, 7.5 + 4.0 * k as f64);
        let step = TAU / m as f64;
        // Meio passo de giro tira os nós da linha do centro e intercala as
        // linhas de um anel com as do vizinho.
        let half = (m % 4 == 0) != (k % 2 == 1);
        let start = -FRAC_PI_2 + if half { step / 2.0 } else { 0.0 };
        for i in 0..m {
            let a = start + step * i as f64;
            out.push((rx * a.cos(), ry * a.sin()));
        }
        left -= m;
        k += 1;
    }
    out
}

/// A janela: o passo do zoom e o ponto do mundo no meio da área.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub zoom: usize,
    pub pan: (f64, f64),
}

impl Default for Window {
    fn default() -> Self {
        Window {
            zoom: ZOOM_ONE,
            pan: (0.0, 0.0),
        }
    }
}

impl Window {
    pub fn factor(&self) -> f64 {
        ZOOMS[self.zoom]
    }

    /// O zoom como aparece no rodapé (`1.0x`).
    pub fn label(&self) -> String {
        let z = self.factor();
        if (z * 10.0).fract().abs() < 1e-9 {
            format!("{z:.1}x")
        } else {
            format!("{z:.2}x")
        }
    }

    /// O texto não escala: o zoom muda quanto cada nó mostra.
    pub fn level(&self) -> Level {
        let z = self.factor();
        if z < 0.7 {
            Level::Mark
        } else if z < 1.3 {
            Level::Name
        } else {
            Level::Card
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom + 1).min(ZOOMS.len() - 1);
    }

    pub fn zoom_out(&mut self) {
        self.zoom = self.zoom.saturating_sub(1);
    }

    /// A célula de um ponto do mundo, em colunas e linhas a partir do meio
    /// da área do grafo.
    pub fn cell(&self, p: (f64, f64)) -> (i32, i32) {
        let z = self.factor();
        (
            ((p.0 - self.pan.0) * 2.0 * z).round() as i32,
            ((p.1 - self.pan.1) * z).round() as i32,
        )
    }

    /// Move o centro da janela, sem sair da caixa dos nós mais uma margem.
    pub fn pan_by(&mut self, dx: f64, dy: f64, points: &[(f64, f64)]) {
        let (mut x0, mut x1, mut y0, mut y1) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for &(x, y) in points {
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
        self.pan.0 = (self.pan.0 + dx).clamp(x0 - PAN_MARGIN.0, x1 + PAN_MARGIN.0);
        self.pan.1 = (self.pan.1 + dy).clamp(y0 - PAN_MARGIN.1, y1 + PAN_MARGIN.1);
    }

    /// Arrastar o grafo por `cols` colunas e `rows` linhas.
    pub fn drag(&mut self, cols: i32, rows: i32, points: &[(f64, f64)]) {
        let z = self.factor();
        self.pan_by(-(cols as f64) / (2.0 * z), -(rows as f64) / z, points);
    }
}

/// O estado da tela.
#[derive(Default)]
pub struct GraphView {
    pub rows: Vec<Row>,
    pub dbs: Vec<DbEntry>,
    /// O grupo escolhido, em `rows`.
    pub sel: usize,
    /// O nó em foco no grupo escolhido; `None` é o foco na lista.
    pub focus: Option<usize>,
    pub win: Window,
    pub filter: String,
    /// O filtro está sendo digitado.
    pub typing: bool,
    /// A caixa de detalhes aberta, quando eles não cabem ao lado.
    pub detail: bool,
    /// A última posição do mouse ao arrastar.
    pub drag: Option<(u16, u16)>,
}

impl GraphView {
    /// Um cofre relido: o grupo e o nó em foco continuam pelo nome; se o
    /// grupo sumiu, fica o vizinho na lista.
    pub fn set(&mut self, entries: &[VaultEntry], dbs: Vec<DbEntry>) {
        let group = self.row().map(|r| r.group.clone());
        let name = self.member().map(|m| m.name.clone());
        self.rows = build(entries, &dbs);
        self.dbs = dbs;
        let kept = group.and_then(|g| self.rows.iter().position(|r| r.group == g));
        if kept.is_none() {
            self.win.pan = (0.0, 0.0);
        }
        self.sel = kept.unwrap_or(self.sel.min(self.rows.len().saturating_sub(1)));
        self.focus = match (kept, name) {
            (Some(_), Some(n)) => self
                .row()
                .and_then(|r| r.members.iter().position(|m| m.name == n)),
            _ => None,
        };
        self.fix_sel();
    }

    pub fn row(&self) -> Option<&Row> {
        self.rows.get(self.sel)
    }

    pub fn member(&self) -> Option<&Member> {
        self.row()?.members.get(self.focus?)
    }

    /// As posições dos nós do grupo escolhido.
    pub fn points(&self) -> Vec<(f64, f64)> {
        layout(self.row().map_or(0, |r| r.members.len()))
    }

    /// Os grupos que passam no filtro: pelo nome do domínio ou de um nó.
    pub fn visible(&self) -> Vec<usize> {
        let q = self.filter.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                q.is_empty()
                    || matches!(&r.group, Group::Domain(d) if d.to_lowercase().contains(&q))
                    || r.members.iter().any(|m| m.name.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// O grupo escolhido tem de estar entre os visíveis.
    pub fn fix_sel(&mut self) {
        let vis = self.visible();
        if !vis.contains(&self.sel) {
            self.select(vis.first().copied().unwrap_or(0));
        }
    }

    /// Escolhe um grupo: o foco volta à lista e o centro à origem; o zoom
    /// fica.
    pub fn select(&mut self, i: usize) {
        if i != self.sel {
            self.sel = i;
            self.focus = None;
            self.detail = false;
            self.win.pan = (0.0, 0.0);
        }
    }

    /// O grupo visível anterior (`-1`) ou seguinte (`1`).
    pub fn step_row(&mut self, by: isize) {
        let vis = self.visible();
        let Some(p) = vis.iter().position(|i| *i == self.sel) else {
            return;
        };
        let q = (p as isize + by).clamp(0, vis.len() as isize - 1) as usize;
        self.select(vis[q]);
    }

    /// O nó anterior ou seguinte, em roda; sem foco, o primeiro.
    pub fn step_focus(&mut self, by: isize) {
        let n = self.row().map_or(0, |r| r.members.len()) as isize;
        if n == 0 {
            return;
        }
        self.focus = Some(match self.focus {
            Some(i) => (i as isize + by).rem_euclid(n) as usize,
            None => 0,
        });
    }

    /// Vai ao dono seguinte do segredo em foco, com o segredo ainda em foco.
    /// `false` quando ele não tem outro dono.
    pub fn next_owner(&mut self) -> bool {
        let Some(name) = self.member().map(|m| m.name.clone()) else {
            return false;
        };
        let owners: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                matches!(r.group, Group::Domain(_)) && r.members.iter().any(|m| m.name == name)
            })
            .map(|(i, _)| i)
            .collect();
        let Some(p) = owners.iter().position(|i| *i == self.sel) else {
            return false;
        };
        if owners.len() < 2 {
            return false;
        }
        let next = owners[(p + 1) % owners.len()];
        if !self.visible().contains(&next) {
            self.filter.clear();
        }
        self.select(next);
        self.focus = self.rows[next].members.iter().position(|m| m.name == name);
        true
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        let pts = self.points();
        self.win.pan_by(dx * PAN_STEP, dy * PAN_STEP, &pts);
    }

    pub fn drag_by(&mut self, cols: i32, rows: i32) {
        let pts = self.points();
        self.win.drag(cols, rows, &pts);
    }

    /// O centro de volta à origem e o zoom em 1.0x.
    pub fn reset(&mut self) {
        self.win = Window::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: &str, domains: &[&str]) -> VaultEntry {
        VaultEntry {
            name: name.into(),
            domains: domains.iter().map(|d| d.to_string()).collect(),
            fake: None,
            tools: Vec::new(),
            kind: kind.into(),
            fingerprint: String::new(),
        }
    }

    fn db(name: &str) -> DbEntry {
        DbEntry {
            name: name.into(),
            kind: "postgres".into(),
            host: "db.exemplo.com".into(),
            port: 5432,
            database: "loja".into(),
            user: "ana".into(),
        }
    }

    fn sample() -> Vec<VaultEntry> {
        vec![
            entry(
                "gh-pessoal",
                "",
                &["github.com", "api.github.com", "ghcr.io"],
            ),
            entry("gh-trabalho", "", &["github.com", "api.github.com"]),
            entry("id-github", "ssh", &["github.com"]),
            entry("curinga", "", &["*.github.com"]),
            entry("npm-token", "", &["registry.npmjs.org"]),
        ]
    }

    fn names(r: &Row) -> Vec<&str> {
        r.members.iter().map(|m| m.name.as_str()).collect()
    }

    fn group<'a>(rows: &'a [Row], d: &str) -> &'a Row {
        rows.iter()
            .find(|r| r.group == Group::Domain(d.into()))
            .unwrap()
    }

    #[test]
    fn modelo_por_dominio() {
        let rows = build(&sample(), &[]);
        let doms: Vec<&Group> = rows.iter().map(|r| &r.group).collect();
        assert_eq!(
            doms,
            [
                &Group::Domain("*.github.com".into()),
                &Group::Domain("api.github.com".into()),
                &Group::Domain("ghcr.io".into()),
                &Group::Domain("github.com".into()),
                &Group::Domain("registry.npmjs.org".into()),
            ]
        );
        // Um segredo de três domínios aparece nos três, com os outros dois.
        for d in ["github.com", "api.github.com", "ghcr.io"] {
            let m = group(&rows, d)
                .members
                .iter()
                .find(|m| m.name == "gh-pessoal")
                .unwrap();
            assert_eq!(m.others, 2, "{d}");
        }
        // Padrões diferentes são grupos diferentes.
        assert_eq!(names(group(&rows, "*.github.com")), ["curinga"]);
        assert_eq!(
            names(group(&rows, "github.com")),
            ["gh-pessoal", "gh-trabalho", "id-github"]
        );
        assert_eq!(group(&rows, "github.com").members[2].kind, Kind::Ssh);
    }

    #[test]
    fn grupos_sem_dominio_e_bancos() {
        // Todos com domínio: sem o grupo.
        assert!(
            !build(&sample(), &[])
                .iter()
                .any(|r| r.group == Group::NoDomain)
        );
        // Uma entrada `db` vem sem domínio e não entra em "sem domínio".
        let mut e = sample();
        e.push(entry("loja", "db", &[]));
        let rows = build(&e, &[db("loja")]);
        assert!(!rows.iter().any(|r| r.group == Group::NoDomain));
        let last = rows.last().unwrap();
        assert_eq!(last.group, Group::Dbs);
        assert_eq!(names(last), ["loja"]);
        assert_eq!(last.members[0].kind, Kind::Db);
        assert!(!rows.iter().any(|r| {
            matches!(r.group, Group::Domain(_)) && r.members.iter().any(|m| m.name == "loja")
        }));
        // Um segredo sem domínio: o grupo vem antes dos bancos.
        e.push(entry("stripe-teste", "", &[]));
        let rows = build(&e, &[db("loja")]);
        let n = rows.len();
        assert_eq!(rows[n - 2].group, Group::NoDomain);
        assert_eq!(names(&rows[n - 2]), ["stripe-teste"]);
        assert_eq!(rows[n - 1].group, Group::Dbs);
    }

    #[test]
    fn aneis_crescem_para_fora() {
        for (n, want) in [(0, 0), (1, 1), (8, 1), (9, 2), (20, 2), (21, 3), (36, 3)] {
            assert_eq!(rings(n), want, "{n} nós");
            let p = layout(n);
            assert_eq!(p.len(), n);
            for i in 0..n {
                for j in 0..i {
                    let d = (p[i].0 - p[j].0).abs() + (p[i].1 - p[j].1).abs();
                    assert!(d > 0.5, "{n} nós: {i} e {j} na mesma posição");
                }
            }
        }
        assert_eq!(
            (ring_capacity(0), ring_capacity(1), ring_capacity(2)),
            (8, 12, 16)
        );
        // Nenhum nó do primeiro anel na linha da caixa do centro.
        for n in 1..=8 {
            assert!(layout(n).iter().all(|p| p.1.abs() >= 1.5), "{n} nós");
        }
    }

    #[test]
    fn janela_zoom_e_conversao() {
        let mut w = Window::default();
        assert_eq!((w.factor(), w.level()), (1.0, Level::Name));
        assert_eq!(w.label(), "1.0x");
        assert_eq!(w.cell((0.0, 0.0)), (0, 0));
        assert_eq!(w.cell((3.0, -2.0)), (6, -2));
        w.zoom_in();
        assert_eq!((w.factor(), w.level()), (1.25, Level::Name));
        assert_eq!(w.label(), "1.25x");
        w.zoom_in();
        assert_eq!((w.factor(), w.level()), (1.6, Level::Card));
        w.zoom_in();
        w.zoom_in();
        w.zoom_in();
        assert_eq!(w.factor(), 2.0);
        assert_eq!(w.cell((3.0, -2.0)), (12, -4));
        for _ in 0..10 {
            w.zoom_out();
        }
        assert_eq!((w.factor(), w.level()), (0.5, Level::Mark));
        w.zoom_in();
        assert_eq!((w.factor(), w.level()), (0.64, Level::Mark));
        w.zoom_in();
        assert_eq!((w.factor(), w.level()), (0.8, Level::Name));
    }

    #[test]
    fn janela_movimento() {
        let pts = layout(2);
        let mut w = Window::default();
        // Arrastar 10 colunas para a direita leva o centro 10 colunas.
        w.drag(10, 0, &pts);
        assert_eq!(w.cell((0.0, 0.0)), (10, 0));
        w.drag(0, -3, &pts);
        assert_eq!(w.cell((0.0, 0.0)), (10, -3));
        // O centro não se perde: para na caixa dos nós mais a margem.
        for _ in 0..100 {
            w.pan_by(5.0, 5.0, &pts);
        }
        assert_eq!(w.pan, (PAN_MARGIN.0, 7.5 + PAN_MARGIN.1));
        for _ in 0..100 {
            w.pan_by(-5.0, -5.0, &pts);
        }
        assert_eq!(w.pan, (-PAN_MARGIN.0, -7.5 - PAN_MARGIN.1));
    }

    fn view(entries: &[VaultEntry]) -> GraphView {
        let mut v = GraphView::default();
        v.set(entries, Vec::new());
        v
    }

    fn pick(v: &mut GraphView, d: &str) {
        let i = v
            .rows
            .iter()
            .position(|r| r.group == Group::Domain(d.into()))
            .unwrap();
        v.select(i);
    }

    #[test]
    fn foco_e_outro_dono() {
        let mut v = view(&sample());
        pick(&mut v, "github.com");
        assert!(v.member().is_none());
        v.step_focus(1);
        assert_eq!(v.member().unwrap().name, "gh-pessoal");
        v.step_focus(-1);
        assert_eq!(v.member().unwrap().name, "id-github");
        v.step_focus(1);
        // Os donos de gh-pessoal, em roda, com ele ainda em foco.
        for d in ["api.github.com", "ghcr.io", "github.com"] {
            assert!(v.next_owner());
            assert_eq!(v.row().unwrap().group, Group::Domain(d.into()));
            assert_eq!(v.member().unwrap().name, "gh-pessoal");
        }
        // Um segredo de um só dono não sai do lugar.
        v.step_focus(1);
        v.step_focus(1);
        assert_eq!(v.member().unwrap().name, "id-github");
        assert!(!v.next_owner());
    }

    #[test]
    fn filtro_pelo_dominio_ou_pelo_segredo() {
        let mut v = view(&sample());
        v.filter = "npm".into();
        v.fix_sel();
        assert_eq!(v.visible().len(), 1);
        assert_eq!(
            v.row().unwrap().group,
            Group::Domain("registry.npmjs.org".into())
        );
        v.filter = "GHCR".into();
        v.fix_sel();
        assert_eq!(v.row().unwrap().group, Group::Domain("ghcr.io".into()));
        // O dono seguinte fora do filtro limpa o filtro.
        v.step_focus(1);
        assert!(v.next_owner());
        assert!(v.filter.is_empty());
    }

    #[test]
    fn reler_mantem_o_grupo_ou_o_vizinho() {
        let mut e = sample();
        let mut v = view(&e);
        pick(&mut v, "ghcr.io");
        v.step_focus(1);
        v.win.zoom_in();
        // O segredo sai de ghcr.io: o grupo some, fica o vizinho, sem foco.
        e[0].domains.retain(|d| d != "ghcr.io");
        v.set(&e, Vec::new());
        assert_eq!(v.row().unwrap().group, Group::Domain("github.com".into()));
        assert!(v.focus.is_none());
        assert_eq!(v.win.factor(), 1.25);
        // Um segredo novo no mesmo grupo: grupo e foco continuam pelo nome.
        v.step_focus(1);
        v.step_focus(1);
        assert_eq!(v.member().unwrap().name, "gh-trabalho");
        e.push(entry("aaa", "", &["github.com"]));
        v.set(&e, Vec::new());
        assert_eq!(v.row().unwrap().group, Group::Domain("github.com".into()));
        assert_eq!(v.member().unwrap().name, "gh-trabalho");
        // Cofre vazio.
        v.set(&[], Vec::new());
        assert!(v.row().is_none() && v.member().is_none());
    }
}
