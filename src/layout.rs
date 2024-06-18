use std::collections::HashMap;
use std::iter::once;

const GAPS: &[i16] = &[0, 1, 0, 2, 1, 10, 4, 40, 18];
const GAPS_COMPACT: &[i16] = &[0, 1, 0, 1, 0, 1, 0, 1, 0];

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

    fn concat_grid(mut layouts: Vec<Vec<Self>>, gap_horiz: i16, gap_vert: i16) -> Self {
        for i in 0..layouts[0].len() {
            let max_width = layouts.iter().map(|row| row[i].width).max().unwrap_or(0);
            for j in 0..layouts.len() {
                layouts[j][i].width = max_width;
            }
        }
        Layout::concat_vert(
            layouts
                .iter()
                .map(|row| Layout::concat_horiz(row.to_vec(), gap_horiz))
                .collect(),
            gap_vert,
        )
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

    pub fn make_layout(n: i16, d: u16, compact: bool) -> Layout {
        let gaps = if compact { GAPS_COMPACT } else { GAPS };

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
            let lower = Self::make_layout(n, ((d as i16) - 1) as u16, compact);
            let mut row = vec![];

            for i in once(-n).chain((-n + 1..n).step_by(2)).chain(once(n)) {
                let mut lower = lower.clone().push_all(i).clean(n);
                if i.abs() == n {
                    if d % 2 == 1 {
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
            if d % 2 == 1 {
                Self::concat_horiz(row, gaps[d as usize])
            } else {
                row.reverse();
                Self::concat_vert(row, gaps[d as usize])
            }
        }
    }
}
