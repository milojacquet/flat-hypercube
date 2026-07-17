use crate::puzzle::Axis;
use crate::puzzle::Side;
use crossterm::event::KeyCode;
use crossterm::style::Color;
use rgb2ansi256::rgb_to_ansi256;
use serde::Deserialize;
use serde::Deserializer;
use serde::de::Error;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::num::ParseIntError;
use std::path::Path;

pub const DEFAULT_FILE_PATH_STR: &'static str = "default_prefs.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Prefs {
    pub axes: Vec<PrefAxis>,
    pub keys: Keys,
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

    pub fn max_dim(&self) -> i16 {
        self.axes.len() as i16
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrefSide {
    pub name: char,
    #[serde(deserialize_with = "de_color")]
    pub color: Color,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Keys {
    #[serde(deserialize_with = "de_keycode_map")]
    pub global: HashMap<KeyCode, KeyCommand>,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Layer {
    pub name: String,
    #[serde(deserialize_with = "de_keycode")]
    pub menu: KeyCode,
    #[serde(deserialize_with = "de_keycode_map")]
    pub keys: HashMap<KeyCode, KeyCommand>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyCommand {
    Null,
    KeybindCycle,
    KeybindMenu,
    Undo,
    Redo,
    NextFilter,
    PrevFilter,
    LiveFilterMode,
    ResetMode,
    Save,
    Layer(KeyCommandLayer),
    Section(KeyCommandSection),
    Side(KeyCommandSide),
    Rotate(KeyCommandRotate),
    Handle(KeyCommandHandle),
    Turn(KeyCommandTurn),
    TurnRotate(KeyCommandTurnRotate),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandLayer {
    pub layer: i16,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandSection {
    pub axis: Axis,
    pub direction: Sign,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandSide {
    pub mode: KeyCommandSideMode,
    #[serde(default)]
    pub strict: bool,
    #[serde(deserialize_with = "de_side")]
    pub side: Side,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandRotate {
    pub mode: KeyCommandSideMode,
    #[serde(default)]
    pub strict: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandHandle {
    #[serde(deserialize_with = "de_side")]
    pub side: Side,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandTurn {
    #[serde(deserialize_with = "de_side")]
    pub side: Side,
    pub layer_min: Option<i16>,
    pub layer_max: Option<i16>,
    pub from: Axis,
    pub to: Axis,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyCommandTurnRotate {
    pub from: Axis,
    pub to: Axis,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyCommandSideMode {
    Simple,
    Fixed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyCommandParamSide {
    axis: Axis,
    sign: Sign,
}

// different from default deserialization for Side
fn de_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: Deserializer<'de>,
{
    let x = KeyCommandParamSide::deserialize(deserializer)?;
    Ok(x.axis.match_sign(x.sign.n()))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sign {
    Pos,
    Neg,
}

impl Sign {
    pub fn n(self) -> i16 {
        match self {
            Sign::Pos => 1,
            Sign::Neg => -1,
        }
    }
}

impl<'de> Deserialize<'de> for Sign {
    fn deserialize<D>(deserializer: D) -> Result<Sign, D::Error>
    where
        D: Deserializer<'de>,
    {
        let n = i32::deserialize(deserializer)?;
        match n {
            1 => Ok(Self::Pos),
            -1 => Ok(Self::Neg),
            _ => Err(D::Error::custom("invalid sign")),
        }
    }
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

fn de_keycode_map<'de, D, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<HashMap<KeyCode, T>, D::Error>
where
    D: Deserializer<'de>,
{
    let st = HashMap::<String, T>::deserialize(deserializer)?;
    st.into_iter()
        .map(|(k, v)| Some((str_keycode(&k)?, v)))
        .collect::<Option<_>>()
        .ok_or(D::Error::custom("not key"))
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
