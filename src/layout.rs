use std::collections::HashMap;
use std::iter::once;

const RATIO: i16 = 2; // terminal char aspect ratio: ~0.5 width/height

fn compute_gaps(n: i16, max_d: u16, semi_compact: bool, compact: bool) -> Vec<i16> {
    let max_idx = max_d as usize + 2; // +2 for vertical-mode d+1 access
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

    // C[d] = cavity height, O[d] = outer sticker height (even d only)
    let mut c = vec![0i16; max_idx];
    let mut o = vec![0i16; max_idx];

    c[2] = n;
    o[2] = 1;
    // C(4) = n * C(2) + (n-1) * (2*O(2) + gap[4])
    c[4] = n * c[2] + (n - 1) * (2 * o[2] + gaps[4]);
    o[4] = c[2];

    // accumulator = O(2) + Σ_{even k, 4≤k≤d-4} (O(k) + gap[k])
    let mut acc = o[2];

    for d in (6..max_idx).step_by(2) {
        gaps[d] = gaps[d - 2] + 2 * acc + 1;

        // acc_c = acc + C(d-4) + gap(d-2) = O(2) + Σ_{even k, 4≤k≤d-2} (C(k-2) + gap[k])
        let acc_c = acc + c[d - 4] + gaps[d - 2];
        let t = gaps[d] + 2 * acc_c;
        c[d] = n * c[d - 2] + (n - 1) * t;
        o[d] = c[d - 2];

        acc += c[d - 4] + gaps[d - 2];
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
}
