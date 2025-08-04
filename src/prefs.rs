#![allow(dead_code)]
use crate::BufReader;
use crossterm::style::Color;
use serde::de::Error;
use serde::Deserializer;
use std::fs::File;
use std::num::ParseIntError;
use std::path::Path;

use rgb2ansi256::rgb_to_ansi256;
use serde::Deserialize;

pub const ESCAPE_CODE: char = '⎋';
pub const BACKSPACE_CODE: char = '⌫';
pub const DEFAULT_FILE_PATH_STR: &'static str = "default_prefs.json";

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
        let file = File::open(Path::new(DEFAULT_FILE_PATH_STR))?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
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
