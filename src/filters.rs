use crate::Prefs;

#[derive(Debug, Clone)]
struct FilterSide {
    have: bool,
    color: i16,
}

// filters are of the form F!U+FB
// (true: i16) = must have color i16
// (false: i16) = must not have color i16
// disjunction of conjunctions
#[derive(Debug, Clone)]
pub struct Filter(Vec<Vec<FilterSide>>);

impl Default for Filter {
    fn default() -> Self {
        Filter(vec![vec![]])
    }
}

impl Filter {
    pub fn parse(st: &str, prefs: &Prefs) -> Result<Self, String> {
        let mut filter = Filter(vec![]);

        for tst in st.split('+') {
            let mut filter_sides = vec![];

            let haves: &str;
            let have_nots: &str;
            match tst.trim().split('!').collect::<Vec<_>>()[..] {
                [a] => {
                    haves = a;
                    have_nots = "";
                }
                [a, b] => {
                    haves = a;
                    have_nots = b;
                }
                _ => return Err("too many ! in string".to_string()),
            }

            let mut add_sides = |have_st: &str, have: bool| -> Result<(), String> {
                for ch in have_st.chars() {
                    if ch.is_whitespace() {
                        continue;
                    }

                    if let Some(ind) = prefs.axes.iter().position(|ax| ax.pos.name == ch) {
                        filter_sides.push(FilterSide {
                            have,
                            color: ind as i16,
                        });
                    } else if let Some(ind) = prefs.axes.iter().position(|ax| ax.neg.name == ch) {
                        filter_sides.push(FilterSide {
                            have,
                            color: !(ind as i16),
                        });
                    } else {
                        return Err(format!("invalid character {ch}"));
                    }
                }

                Ok(())
            };

            add_sides(haves, true)?;
            add_sides(have_nots, false)?;

            filter.0.push(filter_sides)
        }

        Ok(filter)
    }
}

impl FilterSide {
    fn matches_stickers(&self, colors: &[i16]) -> bool {
        colors.iter().any(|e| e == &self.color) == self.have
    }
}

impl Filter {
    pub fn matches_stickers(&self, colors: &[i16]) -> bool {
        self.0
            .iter()
            .any(|sides| sides.iter().all(|side| side.matches_stickers(colors)))
    }
}
