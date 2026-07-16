#![allow(dead_code)]
use crate::puzzle::Axis;
use crate::puzzle::Side;
use crossterm::event::KeyCode;
use crossterm::style::Color;
use serde::Deserializer;
use serde::de::Error;
use std::fs::File;
use std::io::BufReader;
use std::num::ParseIntError;
use std::path::Path;

use rgb2ansi256::rgb_to_ansi256;
use serde::Deserialize;

pub const DEFAULT_FILE_PATH_STR: &'static str = "default_prefs.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Prefs {
    pub axes: Vec<PrefAxis>,
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

    pub fn pos_keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.axes.iter().map(|side| side.pos.keys.select)
    }

    pub fn max_dim(&self) -> i16 {
        self.axes.len() as i16
    }

    pub fn max_layers(&self) -> i16 {
        (self.global_keys.layers.len() * 2 + 1) as i16
    }

    pub fn axis_with(&self, f: impl Fn(&PrefAxis) -> bool) -> Option<Axis> {
        Some(Axis(self.axes.iter().position(f)? as i16))
    }

    pub fn axis_prefs(&self, axis: Axis) -> &PrefAxis {
        &self.axes[axis.0 as usize]
    }

    pub fn side_prefs(&self, side: Side) -> &PrefSide {
        let axis_prefs = self.axis_prefs(side.axis());
        if side.is_pos() {
            &axis_prefs.pos
        } else {
            &axis_prefs.neg
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefAxis {
    pub pos: PrefSide,
    pub neg: PrefSide,
    #[serde(deserialize_with = "de_keycode")]
    pub axis_key: KeyCode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefSide {
    pub name: char,
    #[serde(deserialize_with = "de_color")]
    pub color: Color,
    pub keys: Keys,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Keys {
    #[serde(deserialize_with = "de_keycode")]
    pub select: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub side: KeyCode,
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
    #[serde(deserialize_with = "de_vec_keycode")]
    pub layers: Vec<KeyCode>,
    #[serde(deserialize_with = "de_keycode")]
    pub rotate: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub scramble: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub reset: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub keybind_mode: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub axis_mode: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub undo: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub redo: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub next_filter: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub prev_filter: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub live_filter_mode: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub reset_mode: KeyCode,
    #[serde(deserialize_with = "de_keycode")]
    pub save: KeyCode,
    #[serde(deserialize_with = "de_vec_vec_keycode")]
    pub sections: Vec<Vec<KeyCode>>,
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

fn str_keycode(st: &str) -> Option<KeyCode> {
    if st.len() == 1 {
        Some(KeyCode::Char(st.chars().next().unwrap()))
    } else if let Some(suffix) = st.strip_prefix("F")
        && let Ok(n) = suffix.parse()
    {
        Some(KeyCode::F(n))
    } else {
        match st {
            "Backspace" => Some(KeyCode::Backspace),
            "Enter" => Some(KeyCode::Enter),
            "Left" => Some(KeyCode::Left),
            "Right" => Some(KeyCode::Right),
            "Up" => Some(KeyCode::Up),
            "Down" => Some(KeyCode::Down),
            "Home" => Some(KeyCode::Home),
            "End" => Some(KeyCode::End),
            "PageUp" => Some(KeyCode::PageUp),
            "PageDown" => Some(KeyCode::PageDown),
            "Tab" => Some(KeyCode::Tab),
            "BackTab" => Some(KeyCode::BackTab),
            "Delete" => Some(KeyCode::Delete),
            "Insert" => Some(KeyCode::Insert),
            "Null" => Some(KeyCode::Null),
            "Esc" => Some(KeyCode::Esc),
            "CapsLock" => Some(KeyCode::CapsLock),
            "ScrollLock" => Some(KeyCode::ScrollLock),
            "NumLock" => Some(KeyCode::NumLock),
            "PrintScreen" => Some(KeyCode::PrintScreen),
            "Pause" => Some(KeyCode::Pause),
            "Menu" => Some(KeyCode::Menu),
            "KeypadBegin" => Some(KeyCode::KeypadBegin),
            _ => None,
        }
    }
}

fn de_keycode<'de, D>(deserializer: D) -> Result<KeyCode, D::Error>
where
    D: Deserializer<'de>,
{
    let st = String::deserialize(deserializer)?;
    str_keycode(&st).ok_or(D::Error::custom("not key"))
}

fn de_vec_keycode<'de, D>(deserializer: D) -> Result<Vec<KeyCode>, D::Error>
where
    D: Deserializer<'de>,
{
    let st = Vec::<String>::deserialize(deserializer)?;
    st.into_iter()
        .map(|st| str_keycode(&st))
        .collect::<Option<Vec<_>>>()
        .ok_or(D::Error::custom("not key"))
}

fn de_vec_vec_keycode<'de, D>(deserializer: D) -> Result<Vec<Vec<KeyCode>>, D::Error>
where
    D: Deserializer<'de>,
{
    let st = Vec::<Vec<String>>::deserialize(deserializer)?;
    st.into_iter()
        .map(|v| {
            v.into_iter()
                .map(|st| str_keycode(&st))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(D::Error::custom("not key"))
}

fn de_option_keycode<'de, D>(deserializer: D) -> Result<Option<KeyCode>, D::Error>
where
    D: Deserializer<'de>,
{
    let st = Option::<String>::deserialize(deserializer)?;
    match st {
        None => Ok(None),
        Some(st) => Ok(Some(str_keycode(&st).ok_or(D::Error::custom("not key"))?)),
    }
}

pub fn keycode_name(c: KeyCode) -> String {
    match c {
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::F(n) => format!("[F{n}]"),
        _ => format!("[{c}]"),
    }
}

pub fn keycode_name_char(c: KeyCode) -> char {
    match c {
        KeyCode::Char(c) => c,
        _ => '□',
    }
}
