use itertools::Itertools;
use rand::prelude::*;
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Axis(pub i16);

impl Axis {
    pub fn pos_side(self) -> Side {
        Side(self.0)
    }

    pub fn neg_side(self) -> Side {
        Side(self.0).opposite()
    }

    pub fn match_sign(self, sign: i16) -> Side {
        if sign >= 0 {
            self.pos_side()
        } else {
            self.neg_side()
        }
    }

    pub fn in_dimension(self, d: i16) -> bool {
        self.0 < d
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Side(pub i16);

impl Side {
    pub fn opposite(self) -> Self {
        Self(!self.0)
    }

    pub fn axis(self) -> Axis {
        if self.is_pos() {
            Axis(self.0)
        } else {
            Axis(!self.0)
        }
    }

    pub fn is_pos(self) -> bool {
        self.0 >= 0
    }

    pub fn in_dimension(self, d: i16) -> bool {
        self.axis().in_dimension(d)
    }

    pub fn map(self, f: impl Fn(i16) -> i16) -> Self {
        Self(f(self.0))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct Turn {
    #[serde(flatten)]
    pub block: Option<TurnBlock>,
    pub from: Side,
    pub to: Side,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct TurnBlock {
    pub side: Side,
    // inclusive
    pub layer_min: i16,
    pub layer_max: i16,
}

impl TurnBlock {
    pub fn infinite() -> Self {
        Self {
            side: Side(i16::MAX),
            layer_min: -i16::MAX + 1,
            layer_max: i16::MAX - 1,
        }
    }
}

impl Turn {
    pub fn inverse(&self) -> Self {
        Turn {
            block: self.block,
            from: self.to,
            to: self.from,
        }
    }

    pub fn validate(&self) -> Option<()> {
        if self.from.axis() == self.to.axis() {
            return None;
        }
        if let Some(block) = self.block
            && (block.side.axis() == self.from.axis() || block.side.axis() == self.to.axis())
        {
            return None;
        }
        Some(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PositionType {
    Piece,
    Sticker(Side),
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Position(pub Vec<i16>);

impl Position {
    pub fn layer_at_axis(&self, axis: Axis) -> i16 {
        self.0[axis.0 as usize]
    }

    pub fn mut_layer_at_axis(&mut self, axis: Axis) -> &mut i16 {
        &mut self.0[axis.0 as usize]
    }

    pub fn position_type(&self, n: i16) -> Option<PositionType> {
        let mut on_sides = self
            .0
            .iter()
            .enumerate()
            .filter(|&(_ax, layer)| layer.abs() == n)
            .map(|(ax, layer)| Axis(ax as i16).match_sign(*layer));
        let Some(side) = on_sides.next() else {
            return Some(PositionType::Piece);
        };
        match on_sides.next() {
            Some(_) => None,
            None => Some(PositionType::Sticker(side)),
        }
    }

    pub fn piece_body(&self, n: i16) -> Self {
        let mut out = self.clone();
        out.0.iter_mut().for_each(|layer| {
            if *layer == n {
                *layer -= 1;
            } else if *layer == -n {
                *layer += 1;
            }
        });
        out
    }

    pub fn axes(&self) -> impl Iterator<Item = Axis> {
        (0..self.0.len()).map(|ax| Axis(ax as i16))
    }

    pub fn apply_turn(&mut self, turn: Turn) {
        let Turn { block, from, to } = turn;
        let side;
        let mut layer_min;
        let mut layer_max;
        match block {
            Some(block) => {
                side = Some(block.side.axis());
                layer_min = block.layer_min;
                layer_max = block.layer_max;

                if !block.side.is_pos() {
                    layer_min *= -1;
                    layer_max *= -1;
                    std::mem::swap(&mut layer_min, &mut layer_max)
                }
            }
            None => {
                side = None;
                layer_min = -i16::MAX + 1;
                layer_max = i16::MAX - 1;
            }
        }

        let layer_range = layer_min - 1..=layer_max + 1;

        let to_swap = from.is_pos() != to.is_pos();
        let mut from = from.axis();
        let mut to = to.axis();
        if to_swap {
            std::mem::swap(&mut from, &mut to)
        }

        if side.is_none_or(|side| layer_range.contains(&self.layer_at_axis(side))) {
            let from_old = self.layer_at_axis(from);
            let to_old = self.layer_at_axis(to);

            *self.mut_layer_at_axis(from) = -to_old;
            *self.mut_layer_at_axis(to) = from_old;
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Puzzle {
    pub n: i16,
    pub d: i16,
    // map from coordinate vector (only contains -n+1, n-1 every other, and ±n) to side
    #[serde(with = "serde_map")]
    pub stickers: HashMap<Position, Side>,
}

impl Puzzle {
    pub fn make_solved(n: i16, d: i16) -> Puzzle {
        if d == 1 {
            // I think multi_cartesian_product returns empty iterator for the empty product

            return Puzzle {
                n,
                d,
                stickers: HashMap::from([
                    (Position(vec![-n]), Side(0).opposite()),
                    (Position(vec![n]), Side(0).opposite()),
                ]),
            };
        }

        let mut stickers = HashMap::new();
        for (side, coords) in [n, -n].into_iter().cartesian_product(
            (0..d - 1)
                .map(|_| (-n + 1..n).step_by(2))
                .multi_cartesian_product(),
        ) {
            let mut pos = Position(vec![side]);
            pos.0.extend(&coords);
            for f in 0..d {
                stickers.insert(pos.clone(), Axis(f).match_sign(side));
                pos.0.rotate_right(1);
            }
        }
        Puzzle { n, d, stickers }
    }

    // Checks if the puzzle is solved in any orientation
    pub fn is_solved(&self) -> bool {
        let mut side_colors = HashMap::new();
        for (pos, &color) in &self.stickers {
            let Some(PositionType::Sticker(side)) = pos.position_type(self.n) else {
                panic!("should be on a facet")
            };
            let Some(old_color) = side_colors.insert(side, color) else {
                continue;
            };
            if old_color != color {
                return false;
            }
        }
        true
    }

    pub fn turn(&mut self, turn: Turn) {
        let mut new_stickers = HashMap::new();
        for pos in self.stickers.keys() {
            let mut from_pos = pos.clone();
            from_pos.apply_turn(turn.inverse());
            new_stickers.insert(pos.clone(), self.stickers[&from_pos]);
        }
        self.stickers.extend(new_stickers);
    }

    pub fn is_piece(&self, piece: &Position) -> bool {
        matches!(piece.position_type(self.n), Some(PositionType::Piece))
    }

    pub fn is_sticker(&self, piece: &Position) -> bool {
        matches!(piece.position_type(self.n), Some(PositionType::Sticker(_)))
    }

    pub fn is_sticker_or_piece(&self, piece: &Position) -> bool {
        piece.position_type(self.n).is_some()
    }

    pub fn piece_body(&self, piece: &Position) -> Position {
        piece.piece_body(self.n)
    }

    pub fn piece_body_stickers(&self, piece: &Position) -> Vec<Position> {
        let mut colors = vec![];
        for axis in piece.axes() {
            let mut sticker = piece.clone();
            let layer = sticker.mut_layer_at_axis(axis);
            if *layer == self.n - 1 {
                *layer += 1;
            } else if *layer == -(self.n - 1) {
                *layer -= 1;
            } else {
                continue;
            }
            colors.push(sticker.clone());
            if self.n == 1 {
                // the piece of a 1^d has two stickers per axis
                let layer = sticker.mut_layer_at_axis(axis);
                *layer *= -1;
                colors.push(sticker);
            }
        }
        colors
    }

    fn piece_body_sticker_colors(&self, piece: &Position) -> Vec<Side> {
        let mut colors = vec![];
        for axis in piece.axes() {
            let mut sticker = piece.clone();
            let layer = sticker.mut_layer_at_axis(axis);
            if *layer == self.n - 1 {
                *layer += 1;
            } else if *layer == -(self.n - 1) {
                *layer -= 1;
            } else {
                continue;
            }
            colors.push(self.stickers[&sticker]);
            if self.n == 1 {
                // the piece of a 1^d has two stickers per axis
                colors.push(self.stickers[&sticker].opposite());
            }
        }
        colors
    }

    pub fn piece_sticker_colors(&self, piece: &Position) -> Vec<Side> {
        self.piece_body_sticker_colors(&self.piece_body(piece))
    }

    pub fn scramble(&mut self, rng: &mut ThreadRng) {
        for _ in 0..5000 {
            let mut axes: Vec<i16> = (0..self.d).collect();
            axes.shuffle(rng);
            let layer = self.n - 1 - 2 * rng.gen_range(0..self.n);
            self.turn(Turn {
                block: Some(TurnBlock {
                    side: Side(axes[0]),
                    layer_min: layer,
                    layer_max: layer,
                }),
                from: Side(axes[1]),
                to: Side(axes[2]),
            });
        }
    }

    pub fn axes(&self) -> impl Iterator<Item = Axis> + use<> {
        (0..self.d).map(|ax| Axis(ax))
    }
}
