use std::collections::HashMap;
use std::iter::once;

const RATIO: i16 = 2; // terminal char aspect ratio: ~0.5 width/height

fn compute_gaps(n: i16, max_d: u16, semi_compact: bool, compact: bool) -> Vec<i16> {
    let max_idx = (max_d as usize + 2).max(5); // +2 for vertical-mode d+1 access, min 5 for first-5 init
    let mut gaps = vec![0i16; max_idx];

    if compact {
        for d in 0..max_idx {
            gaps[d] = if d % 2 == 0 { 0 } else { 1 };
        }
        return gaps;
    }

    if semi_compact {
        // first 5: [0, 1, 0, 2, 1], then alternating 3, 1, 3, 1...
        gaps[0] = 0;
        gaps[1] = 1;
        gaps[2] = 0;
        gaps[3] = 2;
        gaps[4] = 1;
        for d in 5..max_idx {
            gaps[d] = if d % 2 == 0 { 1 } else { 3 };
        }
        return gaps;
    }

    // Regular mode: first 5 fixed
    gaps[0] = 0;
    gaps[1] = 1;
    gaps[2] = 0;
    gaps[3] = 2;
    gaps[4] = 1;

    if max_idx <= 6 {
        return gaps;
    }

    // C[d] = cavity height (even d), c[0] = 1
    let mut c = vec![0i16; max_idx];
    c[0] = 1;
    c[2] = n;
    // acc = c[0] + Σ_{even k, 4≤k≤d-2} (C(k-2) + gap[k])
    let mut acc = 0;

    for d in (6..max_idx).step_by(2) {
        acc += c[d - 6] + gaps[d - 4];
        c[d - 2] = n * c[d - 4] + (n - 1) * (gaps[d - 2] + 2 * acc);
        gaps[d] = gaps[d - 2] + 2 * acc + 1;
    }

    for d in (5..max_idx.saturating_sub(1)).step_by(2) {
        gaps[d] = gaps[d + 1] * RATIO;
    }

    gaps
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub width: u16,
    pub height: u16,
    pub points: HashMap<(i16, i16), Vec<i16>>,
    pub keybind_hints: HashMap<(i16, i16), Option<i16>>, // None: core, Some(i): side i
}

impl Layout {
    fn new() -> Self {
        Layout {
            width: 0,
            height: 0,
            points: HashMap::new(),
            keybind_hints: HashMap::new(),
        }
    }

    fn squish_right(&mut self) -> &mut Self {
        self.width = (self.points.keys().map(|(x, _y)| x).max().unwrap_or(&-1) + 1) as u16;
        self
    }

    fn squish_bottom(&mut self) -> &mut Self {
        self.height = (self.points.keys().map(|(_x, y)| y).max().unwrap_or(&-1) + 1) as u16;
        self
    }

    pub fn move_right(self, shift: i16) -> Self {
        let mut out = Self::new();
        for ((x, y), val) in &self.points {
            out.points.insert((x + shift, *y), val.to_vec());
        }
        for ((x, y), val) in &self.keybind_hints {
            out.keybind_hints.insert((x + shift, *y), *val);
        }
        out.width = (self.width as i16 + shift) as u16;
        out.height = self.height;
        out
    }

    fn move_down(self, shift: i16) -> Self {
        let mut out = Self::new();
        for ((x, y), val) in &self.points {
            out.points.insert((*x, y + shift), val.to_vec());
        }
        for ((x, y), val) in &self.keybind_hints {
            out.keybind_hints.insert((*x, y + shift), *val);
        }
        out.width = self.width;
        out.height = (self.height as i16 + shift) as u16;
        out
    }

    fn squish_left(self) -> Self {
        let shift = -self.points.keys().map(|(x, _y)| x).min().unwrap_or(&0);
        self.move_right(shift)
    }

    fn squish_top(self) -> Self {
        let shift = -self.points.keys().map(|(_x, y)| y).min().unwrap_or(&0);
        self.move_down(shift)
    }

    fn squish_horiz(self) -> Self {
        let mut out = self.squish_left();
        out.squish_right();
        out
    }

    fn squish_vert(self) -> Self {
        let mut out = self.squish_top();
        out.squish_bottom();
        out
    }

    #[allow(dead_code)]
    fn squish_all(self) -> Self {
        self.squish_horiz().squish_vert()
    }

    fn union(&mut self, other: Self) -> &mut Self {
        self.points.extend(other.points);
        self.keybind_hints.extend(other.keybind_hints);
        self.width = self.width.max(other.width);
        self.height = self.height.max(other.height);
        self
    }

    fn join_horiz(&mut self, other: Self, gap: i16) -> &mut Self {
        self.union(other.move_right(self.width as i16 + gap))
    }

    fn join_vert(&mut self, other: Self, gap: i16) -> &mut Self {
        self.union(other.move_down(self.height as i16 + gap))
    }

    fn concat_horiz(mut layouts: Vec<Self>, gap: i16) -> Self {
        let layouts_rest = layouts.split_off(1);
        let mut out = layouts
            .into_iter()
            .next()
            .expect("should have at least one element");
        for layout in layouts_rest.into_iter() {
            out.join_horiz(layout, gap);
        }
        out
    }

    fn concat_vert(mut layouts: Vec<Self>, gap: i16) -> Self {
        let layouts_rest = layouts.split_off(1);
        let mut out = layouts
            .into_iter()
            .next()
            .expect("should have at least one element");
        for layout in layouts_rest.into_iter() {
            out.join_vert(layout, gap);
        }
        out
    }

    fn clean(mut self, n: i16) -> Self {
        self.points
            .retain(|_key, val| val.iter().filter(|x| x.abs() == n).count() <= 1);
        self
    }

    fn push_all(self, x: i16) -> Self {
        let mut lower = self.clone();
        for (_xy, ref mut pos) in lower.points.iter_mut() {
            pos.push(x);
        }
        lower
    }

    pub fn make_layout(n: i16, d: u16, semi_compact: bool, compact: bool, vertical: bool) -> Layout {
        let gaps = compute_gaps(n, d, semi_compact, compact);
        Self::make_layout_inner(n, d, vertical, &gaps)
    }

    fn make_layout_inner(n: i16, d: u16, vertical: bool, gaps: &[i16]) -> Layout {
        if d == 0 {
            Layout {
                width: 1,
                height: 1,
                points: HashMap::from([((0, 0), vec![])]),
                keybind_hints: if n > 2 {
                    HashMap::from([((0, 0), None)])
                } else {
                    HashMap::new()
                },
            }
        } else if n == 2 {
            Self::make_layout_inner_n2(d, vertical, gaps)
        } else {
            let make_horizontal = d % 2 == 1 && !vertical;

            let lower = Self::make_layout_inner(n, d - 1, false, gaps);
            let mut row = vec![];

            for i in once(-n).chain((-n + 1..n).step_by(2)).chain(once(n)) {
                let mut lower = lower.clone().push_all(i).clean(n);
                if i.abs() == n {
                    if make_horizontal {
                        lower = lower.squish_horiz();
                    } else {
                        lower = lower.squish_vert();
                    }
                }

                lower.keybind_hints.retain(|_pos, side| {
                    let keep;
                    if i == -n + 1 {
                        keep = side.is_none();
                        *side = Some(!((d - 1) as i16));
                    } else if i == n - 1 {
                        keep = side.is_none();
                        *side = Some((d - 1) as i16);
                    } else {
                        keep = i == 0 || i == 1
                    };
                    keep
                });

                row.push(lower);
            }
            let gap_idx = d as usize
                + if vertical && d % 2 == 1 { 1 } else { 0 };
            if make_horizontal {
                Self::concat_horiz(row, gaps[gap_idx])
            } else {
                row.reverse();
                Self::concat_vert(row, gaps[gap_idx])
            }
        }
    }

    fn make_layout_inner_n2(d: u16, vertical: bool, gaps: &[i16]) -> Layout {
        let make_horizontal = d % 2 == 1 && !vertical;
        let lower = Self::make_layout_inner(2, d - 1, false, gaps);
        let mut row = vec![];

        for i in once(-2).chain(once(-1)).chain(once(1)).chain(once(2)) {
            let mut layer = lower.clone().push_all(i).clean(2);
            if i.abs() == 2 {
                if make_horizontal {
                    layer = layer.squish_horiz();
                } else {
                    layer = layer.squish_vert();
                }
            }

            if i == -2 || i == 2 {
                layer.keybind_hints.clear();
            } else if d == 1 {
                for ((x, y), pos) in layer.points.clone().iter() {
                    if !pos.iter().any(|&v| v.abs() == 2) {
                        layer.keybind_hints.insert((*x, *y), Some(if i == -1 { !0 } else { 0 }));
                    }
                }
            } else if d == 2 {
                let ns: Vec<(i16, i16)> = layer
                    .points
                    .iter()
                    .filter(|(_, coords)| !coords.iter().any(|&v| v.abs() == 2))
                    .map(|(&pos, _)| pos)
                    .collect();
                let min_x = ns.iter().map(|p| p.0).min().unwrap();
                let max_x = ns.iter().map(|p| p.0).max().unwrap();
                let row_y = ns[0].1;

                layer.keybind_hints.clear();
                if i == -1 {
                    layer.keybind_hints.insert((min_x, row_y), Some(!1));
                    layer.keybind_hints.insert((max_x, row_y), Some(0));
                } else {
                    layer.keybind_hints.insert((min_x, row_y), Some(!0));
                    layer.keybind_hints.insert((max_x, row_y), Some(1));
                }
            } else {
                if i == -1 {
                    layer.keybind_hints.clear();
                    Self::place_new_hints_n2(&mut layer, (d - 1) as i16, make_horizontal);
                }
            }

            row.push(layer);
        }

        let gap_idx = d as usize + if vertical && d % 2 == 1 { 1 } else { 0 };
        if make_horizontal {
            Self::concat_horiz(row, gaps[gap_idx])
        } else {
            row.reverse();
            Self::concat_vert(row, gaps[gap_idx])
        }
    }

    /// Place both new-axis hints into the i=-1 layer (n=2, d≥3).
    fn place_new_hints_n2(layer: &mut Layout, new_axis: i16, make_horizontal: bool) {
        let mut ns: Vec<(i16, i16)> = layer
            .points
            .iter()
            .filter(|(_, coords)| !coords.iter().any(|&v| v.abs() == 2))
            .map(|(&pos, _)| pos)
            .collect();

        if ns.len() < 2 {
            return;
        }

        if make_horizontal {
            // topmost, then rightmost → top 1×2: left=neg, right=pos
            ns.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
            let (rx, ry) = ns[0];
            layer.keybind_hints.insert((rx - 2, ry), Some(!new_axis));
            layer.keybind_hints.insert((rx, ry), Some(new_axis));
        } else {
            // rightmost, then topmost → right 2×1: top=pos, bottom=neg
            ns.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let (rx, ry) = ns[0];
            layer.keybind_hints.insert((rx, ry), Some(new_axis));
            layer.keybind_hints.insert((rx, ry + 1), Some(!new_axis));
        }
    }
}
