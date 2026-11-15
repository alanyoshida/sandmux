//! Layout de painéis de uma aba (protocolo 32): uma árvore de divisões com as
//! sessões nas folhas. A aba é identificada pela primeira folha.

use serde::{Deserialize, Serialize};

/// Direção de uma divisão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dir {
    /// Lado a lado (a primeira parte à esquerda).
    Side,
    /// Em cima e embaixo (a primeira parte em cima).
    Stack,
}

/// Direção do foco ao procurar o vizinho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toward {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum Pane {
    Leaf {
        id: u32,
    },
    Split {
        dir: Dir,
        /// Parte da primeira metade, em por cento (10 a 90).
        ratio: u16,
        a: Box<Pane>,
        b: Box<Pane>,
    },
}

/// Um retângulo dentro da área da aba.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Um separador: a coluna (lado a lado) ou a linha (em cima e embaixo)
/// entre duas partes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sep {
    pub dir: Dir,
    pub at: Rect,
}

impl Pane {
    pub fn leaf(id: u32) -> Pane {
        Pane::Leaf { id }
    }

    /// As sessões, na ordem (a primeira identifica a aba).
    pub fn leaves(&self) -> Vec<u32> {
        match self {
            Pane::Leaf { id } => vec![*id],
            Pane::Split { a, b, .. } => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    pub fn first(&self) -> u32 {
        match self {
            Pane::Leaf { id } => *id,
            Pane::Split { a, .. } => a.first(),
        }
    }

    pub fn contains(&self, id: u32) -> bool {
        self.leaves().contains(&id)
    }

    /// Divide a folha `target`, com a sessão `new` na parte nova (à direita
    /// ou embaixo). Devolve se achou a folha.
    pub fn split(&mut self, target: u32, dir: Dir, new: u32) -> bool {
        match self {
            Pane::Leaf { id } if *id == target => {
                *self = Pane::Split {
                    dir,
                    ratio: 50,
                    a: Box::new(Pane::leaf(target)),
                    b: Box::new(Pane::leaf(new)),
                };
                true
            }
            Pane::Leaf { .. } => false,
            Pane::Split { a, b, .. } => a.split(target, dir, new) || b.split(target, dir, new),
        }
    }

    /// Tira a folha `id`, colapsando a divisão dela. `None` quando não sobra
    /// nada.
    pub fn remove(self, id: u32) -> Option<Pane> {
        match self {
            Pane::Leaf { id: l } if l == id => None,
            leaf @ Pane::Leaf { .. } => Some(leaf),
            Pane::Split { dir, ratio, a, b } => match (a.remove(id), b.remove(id)) {
                (Some(a), Some(b)) => Some(Pane::Split {
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            },
        }
    }

    /// Os retângulos de cada folha e os separadores, na área dada.
    pub fn rects(&self, area: Rect) -> (Vec<(u32, Rect)>, Vec<Sep>) {
        let mut out = (Vec::new(), Vec::new());
        self.rects_into(area, &mut out);
        out
    }

    fn rects_into(&self, r: Rect, out: &mut (Vec<(u32, Rect)>, Vec<Sep>)) {
        match self {
            Pane::Leaf { id } => out.0.push((*id, r)),
            Pane::Split { dir, ratio, a, b } => {
                let total = match dir {
                    Dir::Side => r.w,
                    Dir::Stack => r.h,
                };
                // Uma célula para o separador; cada parte com ao menos uma.
                let avail = total.saturating_sub(1);
                let first = if avail >= 2 {
                    ((u32::from(avail) * u32::from(*ratio) / 100) as u16).clamp(1, avail - 1)
                } else {
                    avail.min(1)
                };
                let second = avail.saturating_sub(first);
                let (ra, sep, rb) = match dir {
                    Dir::Side => (
                        Rect { w: first, ..r },
                        Rect {
                            x: r.x + first,
                            w: 1.min(total),
                            ..r
                        },
                        Rect {
                            x: r.x + first + 1,
                            w: second,
                            ..r
                        },
                    ),
                    Dir::Stack => (
                        Rect { h: first, ..r },
                        Rect {
                            y: r.y + first,
                            h: 1.min(total),
                            ..r
                        },
                        Rect {
                            y: r.y + first + 1,
                            h: second,
                            ..r
                        },
                    ),
                };
                a.rects_into(ra, out);
                out.1.push(Sep { dir: *dir, at: sep });
                b.rects_into(rb, out);
            }
        }
    }

    /// O painel vizinho de `id` na direção dada, pela geometria: o mais perto
    /// entre os que se sobrepõem na outra direção.
    pub fn neighbor(&self, id: u32, toward: Toward, area: Rect) -> Option<u32> {
        let (rects, _) = self.rects(area);
        let me = rects.iter().find(|(i, _)| *i == id)?.1;
        let overlap = |a0: u16, a1: u16, b0: u16, b1: u16| a0 < b1 && b0 < a1;
        rects
            .iter()
            .filter(|(i, _)| *i != id)
            .filter_map(|(i, r)| {
                let ok = match toward {
                    Toward::Left => r.x + r.w <= me.x && overlap(r.y, r.y + r.h, me.y, me.y + me.h),
                    Toward::Right => {
                        r.x >= me.x + me.w && overlap(r.y, r.y + r.h, me.y, me.y + me.h)
                    }
                    Toward::Up => r.y + r.h <= me.y && overlap(r.x, r.x + r.w, me.x, me.x + me.w),
                    Toward::Down => {
                        r.y >= me.y + me.h && overlap(r.x, r.x + r.w, me.x, me.x + me.w)
                    }
                };
                let dist = match toward {
                    Toward::Left => me.x.saturating_sub(r.x + r.w),
                    Toward::Right => r.x.saturating_sub(me.x + me.w),
                    Toward::Up => me.y.saturating_sub(r.y + r.h),
                    Toward::Down => r.y.saturating_sub(me.y + me.h),
                };
                ok.then_some((dist, *i))
            })
            .min()
            .map(|(_, i)| i)
    }

    /// Aumenta (`delta` positivo) ou diminui o painel `id`, ajustando a
    /// divisão mais próxima que o contém. Devolve se mudou.
    pub fn resize(&mut self, id: u32, delta: i16) -> bool {
        match self {
            Pane::Leaf { .. } => false,
            Pane::Split { ratio, a, b, .. } => {
                if a.resize(id, delta) || b.resize(id, delta) {
                    return true;
                }
                let sign = if a.contains(id) {
                    1
                } else if b.contains(id) {
                    -1
                } else {
                    return false;
                };
                let new = (i32::from(*ratio) + sign * i32::from(delta)).clamp(10, 90) as u16;
                let changed = new != *ratio;
                *ratio = new;
                changed
            }
        }
    }

    /// Confere o layout: ao menos duas folhas, todas diferentes, e as
    /// divisões entre 10% e 90%.
    pub fn check(&self) -> Result<(), String> {
        let l = self.leaves();
        let mut sorted = l.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != l.len() {
            return Err(sandmux_spec::tr!(
                "o layout repete uma sessão",
                "the layout repeats a session"
            )
            .into());
        }
        fn ratios(p: &Pane) -> bool {
            match p {
                Pane::Leaf { .. } => true,
                Pane::Split { ratio, a, b, .. } => {
                    (10..=90).contains(ratio) && ratios(a) && ratios(b)
                }
            }
        }
        if !ratios(self) {
            return Err(sandmux_spec::tr!(
                "uma divisão do layout está fora de 10% a 90%",
                "a layout split is outside 10% to 90%"
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        w: 81,
        h: 24,
    };

    #[test]
    fn dividir_remover_e_retangulos() {
        let mut p = Pane::leaf(1);
        assert!(p.split(1, Dir::Side, 2));
        assert!(p.split(2, Dir::Stack, 3));
        assert!(!p.split(9, Dir::Side, 4));
        assert_eq!(p.leaves(), vec![1, 2, 3]);
        assert_eq!(p.first(), 1);
        let (rects, seps) = p.rects(AREA);
        assert_eq!(
            rects[0],
            (
                1,
                Rect {
                    x: 0,
                    y: 0,
                    w: 40,
                    h: 24
                }
            )
        );
        assert_eq!(
            rects[1],
            (
                2,
                Rect {
                    x: 41,
                    y: 0,
                    w: 40,
                    h: 11
                }
            )
        );
        assert_eq!(
            rects[2],
            (
                3,
                Rect {
                    x: 41,
                    y: 12,
                    w: 40,
                    h: 12
                }
            )
        );
        assert_eq!(seps.len(), 2);
        assert_eq!(
            seps[0].at,
            Rect {
                x: 40,
                y: 0,
                w: 1,
                h: 24
            }
        );
        // Tirar a folha 2 colapsa a divisão de baixo.
        let p = p.remove(2).unwrap();
        assert_eq!(p.leaves(), vec![1, 3]);
        let p = p.remove(1).unwrap();
        assert_eq!(p, Pane::leaf(3));
        assert_eq!(p.remove(3), None);
    }

    #[test]
    fn vizinhos_e_tamanho() {
        let mut p = Pane::leaf(1);
        p.split(1, Dir::Side, 2);
        p.split(2, Dir::Stack, 3);
        assert_eq!(p.neighbor(1, Toward::Right, AREA), Some(2));
        assert_eq!(p.neighbor(3, Toward::Left, AREA), Some(1));
        assert_eq!(p.neighbor(3, Toward::Up, AREA), Some(2));
        assert_eq!(p.neighbor(2, Toward::Down, AREA), Some(3));
        assert_eq!(p.neighbor(1, Toward::Left, AREA), None);
        // Aumentar a 1 mexe na divisão lado a lado; aumentar a 3, na de baixo.
        assert!(p.resize(1, 5));
        let (rects, _) = p.rects(AREA);
        assert_eq!(rects[0].1.w, 44);
        assert!(p.resize(3, 5));
        let (rects, _) = p.rects(AREA);
        assert!(rects[2].1.h > 12, "{rects:?}");
        // No limite, não passa de 90%.
        for _ in 0..20 {
            p.resize(1, 5);
        }
        assert!(p.check().is_ok());
        let dup = Pane::Split {
            dir: Dir::Side,
            ratio: 50,
            a: Box::new(Pane::leaf(1)),
            b: Box::new(Pane::leaf(1)),
        };
        assert!(dup.check().is_err());
    }
}
