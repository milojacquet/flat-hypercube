use crate::Prefs;

pub const DIGITS: &'static str = "0123456789&";

#[derive(Debug, Clone)]
enum FilterSelector {
    Side(i16),   // color
    Type(usize), // number of stickers
}

#[derive(Debug, Clone)]
struct FilterSelectorBool {
    have: bool,
    selector: FilterSelector,
}

// filters are of the form F!U+FB
// (true: i16) = must have color i16
// (false: i16) = must not have color i16
// disjunction of conjunctions
#[derive(Debug, Clone)]
pub struct Filter(Vec<Vec<FilterSelectorBool>>);

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
                        filter_sides.push(FilterSelectorBool {
                            have,
                            selector: FilterSelector::Side(ind as i16),
                        });
                    } else if let Some(ind) = prefs.axes.iter().position(|ax| ax.neg.name == ch) {
                        filter_sides.push(FilterSelectorBool {
                            have,
                            selector: FilterSelector::Side(!(ind as i16)),
                        });
                    } else if let Some(ind) = DIGITS.chars().position(|c| c == ch) {
                        filter_sides.push(FilterSelectorBool {
                            have,
                            selector: FilterSelector::Type(ind),
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

impl FilterSelector {
    fn matches_stickers(&self, colors: &[i16]) -> bool {
        match self {
            FilterSelector::Side(color) => colors.iter().any(|e| e == color),
            FilterSelector::Type(n) => colors.len() == *n,
        }
    }
}

impl Filter {
    pub fn matches_stickers(&self, colors: &[i16]) -> bool {
        self.0.iter().any(|sides| {
            sides
                .iter()
                .all(|side| side.selector.matches_stickers(colors) == side.have)
        })
    }
}
