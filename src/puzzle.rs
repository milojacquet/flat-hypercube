use itertools::Itertools;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Puzzle {
    pub n: i16,
    pub d: u16,
    // map from coordinate vector (only contains -n+1, n-1 every other, and ±n)
    // to side (sides related by ! are opposite)
    pub stickers: HashMap<Vec<i16>, i16>,
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

    #[allow(dead_code)]
    pub fn is_solved(&self) -> bool {
        for (pos, &color) in &self.stickers {
            if color >= 0 && pos[color as usize] != self.n {
                return false;
            } else if color < 0 && pos[!color as usize] != -self.n {
                return false;
            }
        }
        true
    }
}
