#![allow(dead_code)]
use crossterm::style::Color;
use serde::de::Error;
use serde::Deserializer;
use std::num::ParseIntError;

use rgb2ansi256::rgb_to_ansi256;
use serde::Deserialize;

pub const ESCAPE_CODE: char = '⎋';
pub const BACKSPACE_CODE: char = '⌫';
pub const DEFAULT_FILE_PATH_STR: &str = include_str!("../default_prefs.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Prefs {
    pub axes: Vec<Axis>,
    pub global_keys: GlobalKeys,
    pub global_colors: GlobalColors,
    pub damage_repeat: u8,
    pub alert_frames: u8,
}

impl Prefs {
    pub fn load_default() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(serde_json::from_str(DEFAULT_FILE_PATH_STR)?)
    }

    pub fn pos_keys(&self) -> impl Iterator<Item = char> + '_ {
        self.axes.iter().map(|side| side.pos.keys.select)
    }

    pub fn max_dim(&self) -> u16 {
        self.axes.len() as u16
    }

    pub fn max_layers(&self) -> i16 {
        (self.global_keys.layers.len() * 2 + 1) as i16
    }

    pub fn validate(&self) -> Result<(), String> {
        use std::collections::HashSet;

        // Same field across different (axis + direction) must be unique.
        // select and side each span all pos/neg directions; axis_key spans all axes.
        // Different fields (select vs side vs axis_key) never conflict.
        let mut selects = HashSet::new();
        let mut sides = HashSet::new();
        for (i, ax) in self.axes.iter().enumerate() {
            for (dir, keys) in [("pos", &ax.pos.keys), ("neg", &ax.neg.keys)] {
                if keys.select != '∅' && !selects.insert(keys.select) {
                    return Err(format!(
                        "duplicate select key '{0}' in axis {i} {dir}",
                        keys.select
                    ));
                }
                if keys.side != '∅' && !sides.insert(keys.side) {
                    return Err(format!(
                        "duplicate side key '{0}' in axis {i} {dir}",
                        keys.side
                    ));
                }
            }
        }

        let mut axis_keys = HashSet::new();
        for (i, ax) in self.axes.iter().enumerate() {
            if ax.axis_key != '∅' && !axis_keys.insert(ax.axis_key) {
                return Err(format!(
                    "duplicate axis_key '{0}' in axis {i}",
                    ax.axis_key
                ));
            }
        }

        // Any axis key must not conflict with global keys
        let all: HashSet<char> = selects.iter()
            .chain(sides.iter()).chain(axis_keys.iter()).copied().collect();
        let gk = &self.global_keys;
        for (i, &ch) in gk.layers.iter().enumerate() {
            if all.contains(&ch) {
                return Err(format!(
                    "key '{ch}' in layer key[{i}] conflicts with an axis key"
                ));
            }
        }
        for (label, ch) in [
            ("global rotate", gk.rotate),
            ("global scramble", gk.scramble),
            ("global reset", gk.reset),
            ("global keybind_mode", gk.keybind_mode),
            ("global axis_mode", gk.axis_mode),
            ("global undo", gk.undo),
            ("global redo", gk.redo),
            ("global next_filter", gk.next_filter),
            ("global prev_filter", gk.prev_filter),
            ("global live_filter_mode", gk.live_filter_mode),
            ("global reset_mode", gk.reset_mode),
            ("global save", gk.save),
        ] {
            if all.contains(&ch) {
                return Err(format!(
                    "key '{ch}' in {label} conflicts with an axis key"
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Axis {
    pub pos: Side,
    pub neg: Side,
    pub axis_key: char,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Side {
    pub name: char,
    #[serde(deserialize_with = "de_color")]
    pub color: Color,
    pub keys: Keys,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Keys {
    pub select: char,
    pub side: char,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalColors {
    #[serde(deserialize_with = "de_color")]
    pub piece: Color,
    #[serde(deserialize_with = "de_color")]
    pub filtered: Color,
    #[serde(deserialize_with = "de_color")]
    pub alert: Color,
    #[serde(deserialize_with = "de_color")]
    pub clicked: Color,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalKeys {
    pub layers: Vec<char>,
    pub rotate: char,
    pub scramble: char,
    pub reset: char,
    pub keybind_mode: char,
    pub axis_mode: char,
    pub undo: char,
    pub redo: char,
    pub next_filter: char,
    pub prev_filter: char,
    pub live_filter_mode: char,
    pub reset_mode: char,
    pub save: char,
}

fn hex(st: &str) -> Result<Color, ParseIntError> {
    let hex = u32::from_str_radix(&st, 16)?;
    Ok(Color::AnsiValue(rgb_to_ansi256(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        ((hex >> 0) & 0xff) as u8,
    )))
}

fn de_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: Deserializer<'de>,
{
    let st = String::deserialize(deserializer)?;
    hex(&st).map_err(D::Error::custom)
}
