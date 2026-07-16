use crate::puzzle::Position;
use crate::puzzle::Side;
use std::collections::HashMap;
use std::iter::once;

const GAPS: &[i16] = &[0, 1, 0, 2, 1, 10, 4, 40, 18, 160, 72];
const GAPS_COMPACT: &[i16] = &[0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenLocation {
    pub x: i16,
    pub y: i16,
}

impl ScreenLocation {
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    fn shift_x(self, shift: i16) -> Self {
        Self {
            x: self.x + shift,
            y: self.y,
        }
    }

    fn shift_y(self, shift: i16) -> Self {
        Self {
            x: self.x,
            y: self.y + shift,
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub dimensions: ScreenLocation,
    pub points: HashMap<ScreenLocation, Position>,
    pub keybind_hints: HashMap<ScreenLocation, Option<Side>>, // None: core, Some(i): side i
}

impl Layout {
    fn new() -> Self {
        Layout {
            dimensions: ScreenLocation::new(0, 0),
            points: HashMap::new(),
            keybind_hints: HashMap::new(),
        }
    }

    fn squish_right(&mut self) -> &mut Self {
        self.dimensions.x = (self.points.keys().map(|loc| loc.x).max().unwrap_or(-1) + 1) as i16;
        self
    }

    fn squish_bottom(&mut self) -> &mut Self {
        self.dimensions.y = (self.points.keys().map(|loc| loc.y).max().unwrap_or(-1) + 1) as i16;
        self
    }

    pub fn move_right(self, shift: i16) -> Self {
        let mut out = Self::new();
        for (loc, val) in &self.points {
            out.points.insert(loc.shift_x(shift), val.clone());
        }
        for (loc, val) in &self.keybind_hints {
            out.keybind_hints.insert(loc.shift_x(shift), *val);
        }
        out.dimensions = self.dimensions.shift_x(shift);
        out
    }

    fn move_down(self, shift: i16) -> Self {
        let mut out = Self::new();
        for (loc, val) in &self.points {
            out.points.insert(loc.shift_y(shift), val.clone());
        }
        for (loc, val) in &self.keybind_hints {
            out.keybind_hints.insert(loc.shift_y(shift), *val);
        }
        out.dimensions = self.dimensions.shift_y(shift);
        out
    }

    fn squish_left(self) -> Self {
        let shift = -self.points.keys().map(|loc| loc.x).min().unwrap_or(0);
        self.move_right(shift)
    }

    fn squish_top(self) -> Self {
        let shift = -self.points.keys().map(|loc| loc.y).min().unwrap_or(0);
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

    fn combine(&mut self, other: Self) -> &mut Self {
        self.points.extend(other.points);
        self.keybind_hints.extend(other.keybind_hints);
        self.dimensions = self.dimensions.max(other.dimensions);
        self
    }

    fn join_horiz(&mut self, other: Self, gap: i16) -> &mut Self {
        self.combine(other.move_right(self.dimensions.x + gap))
    }

    fn join_vert(&mut self, other: Self, gap: i16) -> &mut Self {
        self.combine(other.move_down(self.dimensions.y + gap))
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
            .retain(|_key, val| val.position_type(n).is_some());
        self
    }

    fn push_all(self, x: i16) -> Self {
        let mut lower = self.clone();
        for (_xy, ref mut pos) in lower.points.iter_mut() {
            pos.0.push(x);
        }
        lower
    }

    pub fn make_layout(n: i16, d: i16, compact: bool, vertical: bool) -> Layout {
        let gaps = if compact { GAPS_COMPACT } else { GAPS };

        if d == 0 {
            Layout {
                dimensions: ScreenLocation::new(1, 1),
                points: HashMap::from([(ScreenLocation::new(0, 0), Position(vec![]))]),
                keybind_hints: if n > 2 {
                    HashMap::from([(ScreenLocation::new(0, 0), None)])
                } else {
                    HashMap::new()
                },
            }
        } else {
            let make_horizontal = d % 2 == 1 && !vertical;

            let lower = Self::make_layout(n, ((d as i16) - 1) as i16, compact, false);
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
                        *side = Some(Side((d - 1) as i16).opposite());
                    } else if i == n - 1 {
                        keep = side.is_none();
                        *side = Some(Side((d - 1) as i16));
                    } else {
                        keep = i == 0 || i == 1
                    };
                    keep
                });

                row.push(lower);
            }
            if make_horizontal {
                Self::concat_horiz(row, gaps[d as usize])
            } else {
                row.reverse();
                Self::concat_vert(row, gaps[d as usize])
            }
        }
    }
}
