use itertools::Itertools;
use rand::prelude::*;
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone)]
pub struct SideTurn {
    pub side: i16,
    pub layer_min: i16,
    pub layer_max: i16,
    pub from: i16,
    pub to: i16,
}

impl SideTurn {
    pub fn inverse(&self) -> Self {
        SideTurn {
            from: self.to,
            to: self.from,
            side: self.side,
            layer_min: self.layer_min,
            layer_max: self.layer_max,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PuzzleTurn {
    pub from: i16,
    pub to: i16,
}

impl PuzzleTurn {
    pub fn inverse(&self) -> Self {
        PuzzleTurn {
            from: self.to,
            to: self.from,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Turn {
    Side(SideTurn),
    Puzzle(PuzzleTurn),
}

impl Turn {
    pub fn inverse(&self) -> Self {
        match self {
            Self::Side(t) => Self::Side(t.inverse()),
            Self::Puzzle(t) => Self::Puzzle(t.inverse()),
        }
    }
}

mod serde_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub(super) fn serialize<K, V, S>(
        value: &HashMap<K, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        K: Serialize,
        V: Serialize,
    {
        value.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub(super) fn deserialize<'de, K, V, D>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
    where
        D: Deserializer<'de>,
        K: Deserialize<'de> + std::hash::Hash + Eq,
        V: Deserialize<'de>,
    {
        Ok(HashMap::from_iter(<Vec<(K, V)>>::deserialize(
            deserializer,
        )?))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Puzzle {
    pub n: i16,
    pub d: u16,
    // map from coordinate vector (only contains -n+1, n-1 every other, and ±n)
    // to side (sides related by ! are opposite)
    #[serde(with = "serde_map")]
    pub stickers: HashMap<Vec<i16>, i16>,
}

pub fn ax(s: i16) -> i16 {
    s.max(!s)
}

impl Puzzle {
    pub fn make_solved(n: i16, d: u16) -> Puzzle {
        if d == 1 {
            // i think multi_cartesian_product returns empty iterator for the empty product

            return Puzzle {
                n,
                d,
                stickers: HashMap::from([(vec![-n], !0), (vec![n], 0)]),
            };
        }

        let mut stickers = HashMap::new();
        for (side, coords) in [n, -n].into_iter().cartesian_product(
            (0..d - 1)
                .map(|_| (-n + 1..n).step_by(2))
                .multi_cartesian_product(),
        ) {
            let mut pos = vec![side];
            pos.extend(&coords);
            for f in 0..(d as i16) {
                stickers.insert(pos.clone(), if side >= 0 { f } else { !f });
                pos.rotate_right(1);
            }
        }
        Puzzle { n, d, stickers }
    }

    pub fn is_solved(&self) -> bool {
        let mut side_colors = HashMap::new();
        for (pos, &color) in &self.stickers {
            let side = pos
                .iter()
                .position(|x| x.abs() == self.n)
                .expect("should be on a face");
            let side = if pos[side] < 0 { !side } else { side };
            let old_color = side_colors.insert(side, color);
            match old_color {
                Some(c) if c != color => return false,
                _ => (),
            }
        }
        true
    }

    fn side_turn(&mut self, turn: SideTurn) -> Option<()> {
        let SideTurn {
            side,
            layer_min,
            layer_max,
            mut from,
            mut to,
        } = turn;
        if side == from || side == !from || side == to || side == !to || from == to || from == !to {
            return None;
        }

        let layer_range = layer_min - 1..=layer_max + 1;

        let to_swap = (from < 0) != (to < 0);
        if from < 0 {
            from = !from
        }
        if to < 0 {
            to = !to
        }
        if to_swap {
            std::mem::swap(&mut from, &mut to)
        }

        let mut new_stickers = HashMap::new();
        for pos in self.stickers.keys() {
            if (side >= 0 && layer_range.contains(&pos[side as usize]))
                || (side < 0 && layer_range.contains(&pos[(!side) as usize]))
            {
                let mut from_pos = pos.clone();
                from_pos[from as usize] = pos[to as usize];
                from_pos[to as usize] = -pos[from as usize];
                new_stickers.insert(pos.clone(), self.stickers[&from_pos]);
            }
        }
        self.stickers.extend(new_stickers);
        Some(())
    }

    fn puzzle_rotate(&mut self, turn: PuzzleTurn) -> Option<()> {
        let PuzzleTurn { from, to } = turn;
        if from == to || from == !to {
            return None;
        }

        let mut new_stickers = HashMap::new();
        for pos in self.stickers.keys() {
            let mut from_pos = pos.clone();
            from_pos[from as usize] = pos[to as usize];
            from_pos[to as usize] = -pos[from as usize];
            new_stickers.insert(pos.clone(), self.stickers[&from_pos]);
        }
        self.stickers = new_stickers;
        Some(())
    }

    pub fn turn(&mut self, turn: Turn) -> Option<()> {
        match turn {
            Turn::Side(t) => self.side_turn(t),
            Turn::Puzzle(t) => self.puzzle_rotate(t),
        }
    }

    fn piece_body(&self, piece: &[i16]) -> Vec<i16> {
        if let Some(ind) = piece.iter().position(|x| x.abs() == self.n) {
            let mut piece_body = piece.to_vec();
            if piece[ind] == self.n {
                piece_body[ind] -= 1;
            } else {
                piece_body[ind] += 1;
            }
            piece_body
        } else {
            piece.to_vec()
        }
    }

    fn piece_body_stickers(&self, piece: &[i16]) -> Vec<i16> {
        let mut colors = vec![];
        for (ind, x) in piece.iter().enumerate() {
            let mut piece = piece.to_vec();
            if *x == self.n - 1 {
                piece[ind] += 1;
            } else if *x == -(self.n - 1) {
                piece[ind] -= 1;
            } else {
                continue;
            }
            colors.push(self.stickers[&piece]);
            if self.n == 1 {
                // the piece of a 1^d has two stickers per axis
                colors.push(!self.stickers[&piece]);
            }
        }
        colors
    }

    pub fn stickers(&self, piece: &[i16]) -> Vec<i16> {
        self.piece_body_stickers(&self.piece_body(piece))
    }

    pub fn scramble(&mut self, rng: &mut ThreadRng) {
        for _ in 0..5000 {
            let mut axes: Vec<i16> = (0..self.d as i16).collect();
            axes.shuffle(rng);
            let layer = self.n - 1 - 2 * rng.gen_range(0..self.n);
            self.turn(Turn::Side(SideTurn {
                side: axes[0],
                layer_min: layer,
                layer_max: layer,
                from: axes[1],
                to: axes[2],
            }));
        }
    }
}
