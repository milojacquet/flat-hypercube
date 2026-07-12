use crate::filters;
use crate::filters::Filter;
use crate::layout::Layout;
use crate::prefs::{self, keycode_name_char};
use crate::prefs::{Prefs, keycode_name};
use crate::puzzle::{Puzzle, PuzzleTurn, SideTurn, Turn, ax};
use clap::Parser;
use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    },
    style::{self, Stylize},
    terminal,
};
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const FRAME_LENGTH: Duration = Duration::from_millis(1000 / 30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TurnLayer {
    Layer(i16),
    WholePuzzle,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TurnBuild {
    pub layer: Option<TurnLayer>,
    pub side: Option<i16>,
    pub from: Option<i16>,
    pub fixed: Vec<i16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeybindAxial {
    Axial, // select axes, fewer keys
    Side,  // select sides, more keys
}

impl KeybindAxial {
    fn next(&self) -> Self {
        match self {
            Self::Axial => Self::Side,
            Self::Side => Self::Axial,
        }
    }

    fn name(&self) -> String {
        match self {
            Self::Axial => "axis keybinds".to_string(),
            Self::Side => "side keybinds".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeybindSet {
    ThreeKey,       // MC7D, works in d dimensions, depends on axial flag
    ThreeKeyStrict, // MC7D, works in d dimensions, only uses one set of keys for the facets
    FixedKey,       // works in d dimensions, requires d-2 keypresses, depends on axial flag
                    // has addition inversion keys in 3d
                    //XyzKey, // HSC, 4d only
}

impl KeybindSet {
    fn valid(&self, n: i16) -> bool {
        match self {
            Self::ThreeKey => true,
            Self::ThreeKeyStrict => true,
            Self::FixedKey => n >= 3,
            //Self::XyzKey => n == 4,
        }
    }

    fn next(&self, n: i16) -> Self {
        let next = match self {
            Self::ThreeKey => Self::ThreeKeyStrict,
            Self::ThreeKeyStrict => Self::FixedKey,
            Self::FixedKey => Self::ThreeKey, //Self::XyzKey,
                                              //Self::XyzKey => Self::ThreeKey,
        };
        if !next.valid(n) { next.next(n) } else { next }
    }

    fn name(&self) -> String {
        match self {
            Self::ThreeKey => "three-key".to_string(),
            Self::ThreeKeyStrict => "three-key strict".to_string(),
            Self::FixedKey => "fixed-key".to_string(),
            //Self::XyzKey => "xyz".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AppMode {
    #[default]
    Turn,
    LiveFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClickedStyle {
    Clicked,
    OnPiece,
    Hovered,
}

static CLICKED_STYLES: &[ClickedStyle] = &[
    ClickedStyle::Hovered,
    ClickedStyle::OnPiece,
    ClickedStyle::Clicked,
];

impl ClickedStyle {
    fn open(self) -> char {
        match self {
            ClickedStyle::Clicked => '[',
            ClickedStyle::OnPiece => '‹',
            ClickedStyle::Hovered => ' ',
        }
    }

    fn close(self) -> char {
        match self {
            ClickedStyle::Clicked => ']',
            ClickedStyle::OnPiece => '›',
            ClickedStyle::Hovered => ' ',
        }
    }
}

pub struct AppState {
    pub puzzle: Puzzle,
    pub scramble: Puzzle,
    pub mode: AppMode,
    pub current_keys: Vec<KeyCode>,
    pub current_turn: TurnBuild,
    pub alert: u8,
    pub damage_counter: Option<(KeyCode, u8)>,
    pub rng: ThreadRng,
    pub keybind_set: KeybindSet,
    pub keybind_axial: KeybindAxial,
    pub message: Option<String>,
    pub undo_history: Vec<Turn>,
    pub redo_history: Vec<Turn>,
    pub filters: Vec<Filter>,
    pub filter_ind: usize,
    pub use_live_filter: bool,
    pub live_filter_string: String,
    pub live_filter_pending: Filter,
    pub live_filter: Filter,
    pub hovered: Option<(i16, i16)>,
    pub clicked: Vec<Vec<i16>>,
    pub section: Vec<i16>,
    pub filename: PathBuf,
    pub prefs: Prefs,
}

#[derive(Serialize, Deserialize)]
struct AppLog {
    scramble: Puzzle,
    moves: Vec<Turn>,
}

impl AppState {
    fn new(n: Option<i16>, d: Option<u16>, prefs: Prefs) -> Result<Self, String> {
        let Some(n) = n else {
            return Err("n must be specified".into());
        };
        let Some(d) = d else {
            return Err("d must be specified".into());
        };

        if d > prefs.max_dim() {
            return Err(format!(
                "dimension should be less than or equal to {}",
                prefs.max_dim()
            )
            .into());
        }
        if d < 1 {
            return Err("dimension should be greater than 0".into());
        }
        if n > prefs.max_layers() {
            return Err(format!(
                "side should be less than or equal to {}",
                prefs.max_layers()
            )
            .into());
        }
        if d < 1 {
            return Err("side should be greater than 0".into());
        }

        Ok(Self {
            puzzle: Puzzle::make_solved(n, d),
            scramble: Puzzle::make_solved(n, d),
            mode: Default::default(),
            current_keys: Vec::new(),
            current_turn: Default::default(),
            alert: Default::default(),
            damage_counter: Default::default(),
            rng: rand::thread_rng(),
            keybind_set: KeybindSet::ThreeKey,
            keybind_axial: KeybindAxial::Axial,
            message: Default::default(),
            undo_history: Default::default(),
            redo_history: Default::default(),
            filters: vec![],
            filter_ind: 0,
            use_live_filter: false,
            live_filter_string: "".to_string(),
            live_filter: Default::default(),
            live_filter_pending: Default::default(),
            hovered: None,
            clicked: Vec::new(),
            section: Vec::new(),
            filename: Self::new_filename(),
            prefs,
        })
    }

    fn set_section(&mut self, section: usize) {
        self.section = vec![(self.puzzle.n + 1) % 2; section];
    }

    fn to_app_log(&self) -> AppLog {
        AppLog {
            scramble: self.scramble.clone(),
            moves: self.undo_history.clone(),
        }
    }

    fn from_app_log(app_log: AppLog, prefs: Prefs) -> Self {
        let mut state = AppState::new(Some(app_log.scramble.n), Some(app_log.scramble.d), prefs)
            .expect("valid log");
        state.scramble = app_log.scramble.clone();
        state.puzzle = app_log.scramble;
        state.undo_history = app_log.moves.clone();
        for mov in app_log.moves {
            state.puzzle.turn(mov);
        }
        state
    }

    fn new_filename() -> PathBuf {
        use chrono::prelude::*;

        let now: DateTime<Local> = std::time::SystemTime::now().into();
        PathBuf::from(format!(
            "logs/{}.log",
            now.naive_local().format("%Y-%m-%d_%H-%M-%S")
        ))
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let app_log = self.to_app_log();

        if let Some(parent) = self.filename.parent() {
            std::fs::create_dir_all(parent)?
        };
        let file = File::create(self.filename.clone())?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, &app_log)?;
        writer.flush()?;
        Ok(())
    }

    fn flush_modes(&mut self) {
        self.current_keys = Vec::new();
        self.current_turn = Default::default();
        self.live_filter_string = Default::default();
    }

    // for use in three-key strict mode
    fn awaiting_side_as_axis(&self) -> bool {
        self.keybind_set == KeybindSet::ThreeKeyStrict && self.current_turn.side.is_some()
    }

    fn get_side(&self, c: KeyCode) -> Option<i16> {
        self.prefs
            .axes
            .iter()
            .position(|ax| ax.pos.keys.select == c)
            .map(|s| s as i16)
            .or_else(|| {
                self.prefs
                    .axes
                    .iter()
                    .position(|ax| ax.neg.keys.select == c)
                    .map(|s| !(s as i16))
            })
    }

    pub fn make_layout(&self, compact: bool, vertical: bool) -> Layout {
        Layout::make_layout(
            self.puzzle.n,
            self.puzzle.d - self.section.len() as u16,
            compact,
            vertical,
        )
        .move_right(1)
    }

    pub fn process_key(&mut self, c: KeyCode) {
        self.message = None;
        if c == self.prefs.global_keys.scramble || c == self.prefs.global_keys.reset {
            match self.damage_counter {
                None => self.damage_counter = Some((c, 1)),
                Some((ch, i)) if ch == c => {
                    self.damage_counter = Some((c, i + 1));
                }
                _ => (),
            }
        } else {
            self.damage_counter = None;
        }

        if let Some((ch, dr)) = self.damage_counter {
            if dr == self.prefs.damage_repeat {
                self.flush_modes();
                if ch == self.prefs.global_keys.scramble && self.puzzle.d >= 3 {
                    self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                    self.puzzle.scramble(&mut self.rng);
                    self.message = Some("scrambled with 5000 turns".to_string());
                    self.scramble = self.puzzle.clone();
                    self.undo_history = vec![];
                    self.redo_history = vec![];
                } else if ch == self.prefs.global_keys.reset {
                    self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                    self.message = Some("puzzle reset".to_string());
                    self.scramble = self.puzzle.clone();
                    self.undo_history = vec![];
                    self.redo_history = vec![];
                }
                self.damage_counter = None;
            }
        } else if c == self.prefs.global_keys.reset_mode {
            self.mode = Default::default();
            self.flush_modes();
            self.message = None;
        } else if c == self.prefs.global_keys.live_filter_mode
            && !matches!(self.mode, AppMode::LiveFilter)
        {
            self.mode = AppMode::LiveFilter;
        } else if c == self.prefs.global_keys.save {
            match self.save() {
                Ok(()) => self.message = Some(format!("saved to {}", self.filename.display())),
                //Err(err) => self.message = Some(format!("could not save: {}", err)),
                Err(_err) => self.message = Some("could not save".to_string()),
            }
        } else if let Some((d, i)) = self
            .prefs
            .global_keys
            .sections
            .iter()
            .enumerate()
            .filter_map(|(d, ks)| Some((d, ks.iter().position(|k| *k == c)?)))
            .next()
        {
            let sign = (i as i16).min(1) * 2 - 1;
            if let Some(sec) = self.section.get_mut(d) {
                *sec *= sign;
                *sec += if *sec == -self.puzzle.n || *sec == self.puzzle.n - 1 {
                    1
                } else if *sec == self.puzzle.n {
                    0
                } else {
                    2
                };
                *sec *= sign;

                self.message = Some(format!(
                    "section: [{}]",
                    self.section
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        } else {
            match self.mode {
                AppMode::Turn => {
                    let mut just_pressed_side = false;

                    if c == self.prefs.global_keys.keybind_mode {
                        self.flush_modes();
                        self.keybind_set = self.keybind_set.next(self.puzzle.n);
                        self.message = Some(format!("set keybinds to {}", self.keybind_set.name()))
                    } else if c == self.prefs.global_keys.axis_mode {
                        if self.puzzle.d > 6 {
                            self.message = Some("not enough room for side keybinds".to_string());
                        } else {
                            self.flush_modes();
                            self.keybind_axial = self.keybind_axial.next();
                            self.message =
                                Some(format!("set axis mode to {}", self.keybind_axial.name()))
                        }
                    } else if c == self.prefs.global_keys.undo {
                        self.flush_modes();
                        let undid = self.undo_history.pop();
                        match undid {
                            None => {
                                self.message = Some("nothing to undo".to_string());
                            }
                            Some(undid) => {
                                self.puzzle.turn(undid.inverse());
                                self.redo_history.push(undid)
                            }
                        }
                    } else if c == self.prefs.global_keys.redo {
                        self.flush_modes();
                        let redid = self.redo_history.pop();
                        match redid {
                            None => {
                                self.message = Some("nothing to redo".to_string());
                            }
                            Some(redid) => {
                                self.puzzle.turn(redid.clone());
                                self.undo_history.push(redid)
                            }
                        }
                    } else if c == self.prefs.global_keys.next_filter {
                        if self.filters.is_empty() {
                            self.message = Some("no filters loaded".to_string());
                        } else {
                            self.flush_modes();
                            self.filter_ind += 1;
                            self.use_live_filter = false;
                            self.message = Some("next filter".to_string());
                        }
                    } else if c == self.prefs.global_keys.prev_filter {
                        if self.filters.is_empty() {
                            self.message = Some("no filters loaded".to_string());
                        } else {
                            self.flush_modes();
                            self.filter_ind -= 1;
                            self.use_live_filter = false;
                            self.message = Some("previous filter".to_string());
                        }
                    } else if let Some(s) =
                        self.prefs.global_keys.layers.iter().position(|ch| ch == &c)
                    {
                        if s as i16 >= self.puzzle.n {
                            return;
                        }
                        self.flush_modes();
                        self.current_keys.push(c);
                        self.current_turn.layer = Some(TurnLayer::Layer(s as i16));
                    } else if !self.awaiting_side_as_axis()
                        && let Some(s) = self.get_side(c)
                    {
                        if s.max(!s) as u16 >= self.puzzle.d {
                            return;
                        }
                        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                            self.flush_modes();
                        }
                        self.current_keys.push(c);
                        self.current_turn.side = Some(s);
                        just_pressed_side = true;
                    } else if c == self.prefs.global_keys.rotate {
                        if self.keybind_set == KeybindSet::ThreeKey {
                            self.flush_modes();
                            just_pressed_side = true;
                        }
                        self.current_keys.push(c);
                        self.current_turn.layer = Some(TurnLayer::WholePuzzle);
                    }

                    match self.keybind_set {
                        KeybindSet::ThreeKey | KeybindSet::ThreeKeyStrict => {
                            let strict = self.keybind_set == KeybindSet::ThreeKeyStrict;

                            let axis = if strict {
                                self.get_side(c)
                            } else {
                                self.get_axis_key(c)
                            };

                            if (self.current_turn.side.is_some()
                                || self.current_turn.layer == Some(TurnLayer::WholePuzzle))
                                && !(strict && just_pressed_side)
                                && let Some(s) = axis
                            {
                                if ax(s) as u16 >= self.puzzle.d {
                                    return;
                                }
                                self.current_keys.push(c);

                                let side = if self.current_turn.side.is_some() {
                                    self.current_turn.side
                                } else if self.current_turn.layer == Some(TurnLayer::WholePuzzle) {
                                    Some(0) // dummy value
                                } else {
                                    None
                                };

                                if let Some(side) = side {
                                    if let Some(from) = self.current_turn.from {
                                        let turn_out = self.perform_turn(side, from, s);

                                        if turn_out.is_none() {
                                            self.alert = self.prefs.alert_frames * 4 - 1;
                                            let key_removal = if strict { 3 } else { 2 };
                                            self.current_keys = self.current_keys
                                                [..self.current_keys.len() - key_removal]
                                                .to_vec();
                                        }
                                        self.current_turn.from = None;
                                        if strict {
                                            self.current_turn.side = None;
                                        }
                                    } else {
                                        self.current_turn.from = Some(s);
                                    }
                                }
                            }
                        }
                        KeybindSet::FixedKey if self.puzzle.d == 3 => {
                            let flip;
                            if let Some(s) =
                                self.prefs.axes.iter().position(|ax| ax.pos.keys.side == c)
                            {
                                if ax(s as i16) as u16 >= self.puzzle.d {
                                    return;
                                }
                                if self.current_turn.layer.is_none()
                                    || self.current_turn.side.is_some()
                                {
                                    self.flush_modes();
                                }
                                self.current_keys.push(c);
                                self.current_turn.side = Some(s as i16);
                                flip = true;
                                just_pressed_side = true;
                            } else if let Some(s) =
                                self.prefs.axes.iter().position(|ax| ax.neg.keys.side == c)
                            {
                                if ax(s as i16) as u16 >= self.puzzle.d {
                                    return;
                                }
                                if self.current_turn.layer.is_none()
                                    || self.current_turn.side.is_some()
                                {
                                    self.flush_modes();
                                }
                                self.current_keys.push(c);
                                self.current_turn.side = Some(!(s as i16));
                                flip = true;
                                just_pressed_side = true;
                            } else {
                                flip = false;
                            }

                            if let (Some(side), true) = (self.current_turn.side, just_pressed_side)
                            {
                                if flip {
                                    if side < 0 {
                                        self.perform_turn(side, (!side + 1) % 3, (!side + 2) % 3);
                                    } else {
                                        self.perform_turn(side, (side + 2) % 3, (side + 1) % 3);
                                    }
                                } else if side < 0 {
                                    self.perform_turn(side, (!side + 2) % 3, (!side + 1) % 3);
                                } else {
                                    self.perform_turn(side, (side + 1) % 3, (side + 2) % 3);
                                }
                            }
                        }
                        KeybindSet::FixedKey => {
                            let axis = self.get_axis_key(c);

                            if let Some(s) = axis {
                                if ax(s) as u16 >= self.puzzle.d {
                                    return;
                                }
                                self.current_keys.push(c);
                                self.current_turn.fixed.push(s);

                                if let Some(side) = self.current_turn.side {
                                    if self.current_turn.fixed.len() == self.puzzle.d as usize - 3 {
                                        let mut sign = true;
                                        let mut axes = vec![side];
                                        axes.extend(self.current_turn.fixed.iter().cloned());

                                        for axis in &mut axes {
                                            if *axis < 0 {
                                                sign = !sign;
                                                *axis = !*axis;
                                            }
                                        }
                                        //self.message = format!("{:?}", axes).into();

                                        for axis in 0..self.puzzle.d as i16 {
                                            if !axes.contains(&axis) {
                                                axes.push(axis);
                                            }
                                        }

                                        let mut turn_out = Some(()); // i wish we had try blocks

                                        if axes.len() > self.puzzle.d as usize {
                                            // there was a duplicate in axes
                                            turn_out = None;
                                        }

                                        let turn_out = turn_out.and_then(|_| {
                                            for i in 0..axes.len() {
                                                for j in 0..i {
                                                    if i > j {
                                                        sign = !sign;
                                                    }
                                                }
                                            }
                                            let mut from = axes[axes.len() - 2];
                                            let mut to = axes[axes.len() - 1];
                                            if !sign {
                                                std::mem::swap(&mut from, &mut to);
                                            }
                                            self.perform_turn(side, from, to)
                                        });

                                        if turn_out.is_none() {
                                            self.alert = self.prefs.alert_frames * 4 - 1;
                                            self.current_keys =
                                                self.current_keys[..self.current_keys.len()
                                                    - self.current_turn.fixed.len()]
                                                    .to_vec();
                                        }
                                        self.current_turn.fixed = vec![];
                                    }
                                }
                            }
                        } //_ => todo!(),
                    }
                }

                AppMode::LiveFilter => {
                    if let KeyCode::Char(ch) = c
                        && (ch == '+' || ch == '!')
                    {
                        self.live_filter_string.push(ch);
                    } else if let Some((s, side)) = self
                        .prefs
                        .axes
                        .iter()
                        .enumerate()
                        .find_map(|(s, ax)| (ax.pos.keys.select == c).then_some((s, &ax.pos)))
                    {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        self.live_filter_string.push(side.name);
                    } else if let Some((s, side)) = self
                        .prefs
                        .axes
                        .iter()
                        .enumerate()
                        .find_map(|(s, ax)| (ax.neg.keys.select == c).then_some((s, &ax.neg)))
                    {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        self.live_filter_string.push(side.name);
                    } else if let KeyCode::Char(ch) = c
                        && (self
                            .prefs
                            .axes
                            .iter()
                            .any(|ax| ax.pos.name == ch || ax.neg.name == ch))
                    {
                        self.live_filter_string.push(ch);
                    } else if let KeyCode::Char(cc) = c
                        && let Some(ind) = filters::DIGITS.chars().position(|ch| cc == ch)
                    {
                        if ind <= self.puzzle.d as usize {
                            self.live_filter_string.push(cc);
                        }
                    } else if c == KeyCode::Backspace {
                        self.live_filter_string.pop();
                    }

                    let filter_result: Result<Filter, _> =
                        Filter::parse(&self.live_filter_string, &self.prefs);
                    if let Ok(filter) = &filter_result {
                        self.live_filter_pending = filter.clone();
                    }

                    if c == KeyCode::Enter {
                        if let Err(err) = filter_result {
                            self.message = Some(err);
                        } else {
                            self.flush_modes();
                            self.mode = Default::default();
                            self.use_live_filter = true;
                            self.live_filter = self.live_filter_pending.clone();
                        }
                    }
                }
            }
        }
    }

    fn get_axis_key(&self, c: KeyCode) -> Option<i16> {
        if self.keybind_set == KeybindSet::ThreeKeyStrict {
            return None;
        }

        match self.keybind_axial {
            KeybindAxial::Axial => self.prefs.axes.iter().position(|ax| ax.axis_key == c),
            KeybindAxial::Side => self.prefs.axes.iter().enumerate().find_map(|(s, ax)| {
                (ax.pos.keys.side == c)
                    .then_some(s)
                    .or_else(|| (ax.neg.keys.side == c).then_some(!s))
            }),
        }
        .map(|s| s as i16)
    }

    fn perform_turn(&mut self, side: i16, from: i16, to: i16) -> Option<()> {
        let mut layer_min;
        let mut layer_max;
        let turn = match self.current_turn.layer {
            Some(TurnLayer::WholePuzzle) => {
                layer_min = -self.puzzle.n + 1;
                layer_max = self.puzzle.n - 1;
                Turn::Puzzle(PuzzleTurn { from, to })
            }
            _ => {
                match self.current_turn.layer {
                    None => {
                        layer_min = self.puzzle.n - 1;
                        layer_max = self.puzzle.n - 1;
                    }
                    Some(TurnLayer::Layer(l)) => {
                        layer_min = self.puzzle.n - 1 - 2 * l;
                        layer_max = self.puzzle.n - 1 - 2 * l;
                    }
                    Some(TurnLayer::WholePuzzle) => {
                        unreachable!()
                    }
                }
                if side < 0 {
                    layer_min *= -1;
                    layer_max *= -1;
                    std::mem::swap(&mut layer_min, &mut layer_max)
                };
                Turn::Side(SideTurn {
                    side,
                    layer_min,
                    layer_max,
                    from,
                    to,
                })
            }
        };

        // turn clicked stickers
        {
            let mut from = from;
            let mut to = to;
            let mut side = side;
            let to_swap = (from < 0) != (to < 0);
            if from < 0 {
                from = !from
            }
            if to < 0 {
                to = !to
            }
            if side < 0 {
                side = !side
            }
            if to_swap {
                std::mem::swap(&mut from, &mut to)
            }
            for clicked in &mut self.clicked {
                if (layer_min - 1..=layer_max + 1).contains(&clicked[side as usize]) {
                    clicked.swap(from as usize, to as usize);
                    clicked[from as usize] *= -1
                }
            }
        }

        self.undo_history.push(turn.clone());
        let turn_out = self.puzzle.turn(turn);

        if turn_out.is_some() && self.puzzle.is_solved() {
            self.message = Some("solved!".to_string());
        }

        turn_out
    }

    fn get_message(&self) -> String {
        if let Some(message) = &self.message {
            return message.to_string();
        }
        match self.mode {
            AppMode::Turn => self
                .current_keys
                .iter()
                .map(|c| keycode_name(*c))
                .collect::<Vec<_>>()
                .join(""),
            AppMode::LiveFilter => format!("live filter: {}", self.live_filter_string),
        }
    }

    fn clicked_stickers(&self) -> HashMap<Vec<i16>, ClickedStyle> {
        let mut out = HashMap::new();
        for clicked in &self.clicked {
            let body = self.puzzle.piece_body(clicked);
            out.insert(body.clone(), ClickedStyle::OnPiece);
            for i in 0..self.puzzle.d as usize {
                if body[i] == self.puzzle.n - 1 {
                    let mut sticker = body.clone();
                    sticker[i] = self.puzzle.n;
                    out.insert(sticker, ClickedStyle::OnPiece);
                }
                if body[i] == -self.puzzle.n + 1 {
                    let mut sticker = body.clone();
                    sticker[i] = -self.puzzle.n;
                    out.insert(sticker, ClickedStyle::OnPiece);
                }
            }
            out.insert(clicked.clone(), ClickedStyle::Clicked);
        }
        out
    }
}

fn draw_brackets(
    stdout: &mut io::Stdout,
    x: i16,
    y: i16,
    style: ClickedStyle,
    prefs: &Prefs,
) -> Result<(), Box<dyn std::error::Error>> {
    let color = prefs.global_colors.clicked;
    stdout
        .queue(cursor::MoveTo(x as u16 - 1, y as u16))?
        .queue(style::PrintStyledContent(style.open().with(color)))?
        .queue(cursor::MoveTo(x as u16 + 1, y as u16))?
        .queue(style::PrintStyledContent(style.close().with(color)))?;
    Ok(())
}

fn erase_brackets(
    stdout: &mut io::Stdout,
    x: i16,
    y: i16,
) -> Result<(), Box<dyn std::error::Error>> {
    stdout
        .queue(cursor::MoveTo(x as u16 - 1, y as u16))?
        .queue(style::Print(' '))?
        .queue(cursor::MoveTo(x as u16 + 1, y as u16))?
        .queue(style::Print(' '))?;
    Ok(())
}

struct StdoutManager;

impl Drop for StdoutManager {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = stdout.execute(cursor::Show);
        let _ = stdout.execute(crossterm::event::DisableMouseCapture);
        let _ = terminal::disable_raw_mode(); // does this help?
    }
}

/// Flat hypercube simulator
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Number of layers of the puzzle
    n: Option<i16>,
    /// Dimension of the puzzle
    d: Option<u16>,

    /// Display in compact mode
    #[arg(short, long)]
    compact: bool,

    /// File that contains the filters for the solve, one per line
    #[arg(short, long)]
    filters: Option<PathBuf>,

    /// Log file to open
    #[arg(short, long)]
    log: Option<PathBuf>,

    /// Display in vertical mode. This has no effect if d is even.
    #[arg(long)]
    vertical: bool,

    /// Display using colored boxes.
    #[arg(long)]
    boxes: bool,

    /// Number of dimensions to cut by
    #[arg(short, long, default_value("0"))]
    section: usize,

    /// Preferences file
    #[arg(short, long)]
    prefs: Option<PathBuf>,
}

pub fn main_inner() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let prefs: Prefs = {
        let path = args
            .prefs
            .unwrap_or(PathBuf::from(prefs::DEFAULT_FILE_PATH_STR));
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)?
    };

    let mut state;
    if let Some(log_file) = args.log {
        let file = File::open(log_file)?;
        let reader = BufReader::new(file);
        let app_log = serde_json::from_reader(reader).map_err(std::io::Error::other)?;
        state = AppState::from_app_log(app_log, prefs);
    } else {
        state = AppState::new(args.n, args.d, prefs)?;
    }

    state.set_section(args.section);

    if let Some(path) = args.filters {
        let filters_str = std::fs::read_to_string(path).expect("Invalid filter file");
        state.filters = filters_str
            .lines()
            .map(|l| Filter::parse(&l, &state.prefs).unwrap())
            .collect();
    }

    let layout = state.make_layout(args.compact, args.vertical);
    //println!("{:?}", layout.keybind_hints);
    //return Ok(());

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(cursor::Hide)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;

    let stdout_manager = StdoutManager;

    let mut persistent_clicked_locs = HashMap::new();

    'event: loop {
        let previous_message = state.get_message();
        let previous_hovered = state.hovered;
        let mut just_resized = false;

        let frame_begin = Instant::now();

        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    modifiers,
                    ..
                }) => match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        break 'event;
                    }
                    _ => {
                        state.process_key(code);
                    }
                },
                Event::Mouse(MouseEvent {
                    kind, column, row, ..
                }) => {
                    let key = (column as i16, row as i16);
                    let sticker = layout.points.get(&key);
                    if let Some(sticker) = sticker {
                        let mut sticker = sticker.clone();
                        sticker.extend(state.section.iter());
                        let sticker = sticker;

                        match kind {
                            MouseEventKind::Down(_button) => {
                                let original_length = state.clicked.len();
                                state.clicked.retain(|st| {
                                    st.iter()
                                        .zip(sticker.iter())
                                        .any(|(a, b)| (a - b).abs() > 1)
                                });

                                if original_length == state.clicked.len() {
                                    state.clicked.push(sticker.clone());
                                }
                            }
                            MouseEventKind::Moved => {
                                state.hovered = Some(key);
                            }
                            _ => {}
                        }
                    }
                }
                Event::Resize(_, _) => {
                    stdout.execute(terminal::Clear(terminal::ClearType::All))?;
                    just_resized = true;
                }
                _ => (),
            }
        }

        let message = state.get_message();

        if just_resized {
            stdout
                .queue(cursor::MoveTo(0, layout.height))?
                .queue(terminal::Clear(terminal::ClearType::All))?
                .flush()?;
        }
        if previous_message != message {
            stdout
                .queue(cursor::MoveTo(0, layout.height))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .queue(style::Print(message))?;
        }

        if let Some((x, y)) = previous_hovered {
            erase_brackets(&mut stdout, x, y)?;
        }

        if let Some((x, y)) = state.hovered {
            draw_brackets(&mut stdout, x, y, ClickedStyle::Hovered, &state.prefs)?;
        }

        let clicked_stickers = state.clicked_stickers();
        let mut clicked_locs: HashMap<_, _> = CLICKED_STYLES
            .into_iter()
            .map(|s| (*s, HashSet::new()))
            .collect();

        for ((x, y), pos) in &layout.points {
            // in this loop we are more efficient by not flushing the buffer.

            let mut pos = pos.clone();
            pos.extend(state.section.iter());
            if !state.puzzle.is_piece(&pos) {
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::Print(' '))?;
                continue;
            }

            let ch;
            let color;
            let filter = if matches!(state.mode, AppMode::LiveFilter) {
                &state.live_filter_pending
            } else if state.use_live_filter {
                &state.live_filter
            } else if let Some(filter) = state.filters.get(state.filter_ind) {
                filter
            } else {
                &Default::default()
            };

            let in_filter = filter.matches_stickers(&state.puzzle.piece_stickers(&pos));

            if pos.iter().any(|x| x.abs() == state.puzzle.n) {
                let side = state.puzzle.stickers[&pos];
                ch = if args.boxes {
                    '■'
                } else if side >= 0 {
                    state.prefs.axes[side as usize].pos.name
                } else {
                    state.prefs.axes[(!side) as usize].neg.name
                };
                color = if !in_filter {
                    state.prefs.global_colors.filtered
                } else if side >= 0 {
                    state.prefs.axes[side as usize].pos.color
                } else {
                    state.prefs.axes[(!side) as usize].neg.color
                };
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            } else if !matches!(layout.keybind_hints.get(&(*x, *y)), Some(Some(_))) {
                if state.alert % (state.prefs.alert_frames * 2) >= state.prefs.alert_frames {
                    ch = '+';
                    color = state.prefs.global_colors.alert;
                } else {
                    ch = '·';
                    color = if in_filter {
                        state.prefs.global_colors.piece
                    } else {
                        state.prefs.global_colors.filtered
                    };
                }
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }

            if let Some(style) = clicked_stickers.get(&pos) {
                clicked_locs
                    .get_mut(style)
                    .expect("contains")
                    .insert((*x, *y));
            }
        }

        for pcl in persistent_clicked_locs.values() {
            for &(x, y) in pcl {
                erase_brackets(&mut stdout, x, y)?;
            }
        }

        for style in CLICKED_STYLES {
            for (x, y) in clicked_locs.get(style).expect("contains") {
                draw_brackets(&mut stdout, *x, *y, *style, &state.prefs)?;
            }
        }

        for ((x, y), side) in &layout.keybind_hints {
            let mut pos = layout.points[&(*x, *y)].clone();
            pos.extend(state.section.iter());
            if state.puzzle.is_sticker(&pos) {
                continue;
            }

            // in this loop we are more efficient by not flushing the buffer.
            let ch;
            let color;
            if let Some(side) = side {
                ch = if state.current_turn.side.is_none()
                    || (state.keybind_set == KeybindSet::FixedKey && state.puzzle.d == 3)
                    || state.keybind_set == KeybindSet::ThreeKeyStrict
                {
                    keycode_name_char(if *side >= 0 {
                        state.prefs.axes[*side as usize].pos.keys.select
                    } else {
                        state.prefs.axes[(!side) as usize].neg.keys.select
                    })
                } else {
                    match state.keybind_axial {
                        KeybindAxial::Axial => {
                            if *side >= 0 {
                                keycode_name_char(state.prefs.axes[*side as usize].axis_key)
                            } else {
                                '·'
                            }
                        }
                        KeybindAxial::Side => keycode_name_char(if *side >= 0 {
                            state.prefs.axes[*side as usize].pos.keys.side
                        } else {
                            state.prefs.axes[(!side) as usize].neg.keys.side
                        }),
                    }
                };
                color = state.prefs.global_colors.piece;

                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }
            //state.message = format!("{:?}", (x, y, side)).into();
        }

        stdout.queue(cursor::MoveTo(0, layout.height))?.flush()?;

        if state.alert > 0 {
            state.alert -= 1;
        }

        let frame_end = Instant::now();
        let frame = frame_end - frame_begin;
        if frame < FRAME_LENGTH {
            std::thread::sleep(FRAME_LENGTH - frame);
        }
        //state.puzzle.turn(0, 2, 2, 1); // R

        persistent_clicked_locs = clicked_locs;
    }

    drop(stdout_manager);
    Ok(())
}
