use crate::filters;
use crate::filters::Filter;
use crate::layout::Layout;
use crate::prefs::Prefs;
use crate::prefs::BACKSPACE_CODE;
use crate::prefs::ESCAPE_CODE;
use crate::puzzle::{ax, Puzzle, PuzzleTurn, SideTurn, Turn};
use clap::Parser;
use crossterm::{
    cursor,
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    },
    style::{self, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const FRAME_LENGTH: Duration = Duration::from_millis(1000 / 30);

static CTRL_C_PRESSED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TurnLayer {
    Layer(i16),
    WholePuzzle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub ch: char,
    pub axis: i16,
}

#[derive(Debug, Clone, Default)]
pub struct TurnBuild {
    pub layer: Option<TurnLayer>,
    pub layer_char: Option<char>,
    pub side: Option<KeyPress>,
    pub from: Option<KeyPress>,
    pub fixed: Vec<KeyPress>,
}

impl TurnBuild {
    fn current_keys(&self) -> String {
        let mut s = String::new();
        if let Some(ch) = self.layer_char {
            s.push(ch);
        }
        if let Some(kp) = &self.side {
            s.push(kp.ch);
        }
        if let Some(kp) = &self.from {
            s.push(kp.ch);
        }
        for kp in &self.fixed {
            s.push(kp.ch);
        }
        s
    }

    fn clear(&mut self) {
        *self = Default::default();
    }
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
        if !next.valid(n) {
            next.next(n)
        } else {
            next
        }
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
    pub last_turn_keys: String,
    pub current_turn: TurnBuild,
    pub alert: u8,
    pub damage_counter: Option<(char, u8)>,
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
    pub filename: PathBuf,
    pub prefs: Prefs,
    pub rev_stack: Vec<RevEntry>,
}

#[derive(Debug, Clone)]
pub struct RevEntry {
    pub start: usize,
    pub end: Option<usize>,
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
                "dimension {} exceeds the {} axes defined in this prefs config",
                d,
                prefs.max_dim()
            )
            .into());
        }
        if d < 1 {
            return Err("dimension should be greater than 0".into());
        }
        if n > prefs.max_layers() {
            return Err(format!(
                "layers {} exceeds the {} layer keys defined in this prefs config",
                n,
                prefs.max_layers()
            )
            .into());
        }
        if n < 1 {
            return Err("layers should be greater than 0".into());
        }

        Ok(Self {
            puzzle: Puzzle::make_solved(n, d),
            scramble: Puzzle::make_solved(n, d),
            mode: Default::default(),
            last_turn_keys: String::new(),
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
            filename: Self::new_filename(),
            prefs,
            rev_stack: vec![],
        })
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
        self.current_turn.clear();
        self.last_turn_keys.clear();
        self.live_filter_string = Default::default();
    }

    // for use in three-key strict mode
    fn awaiting_side_as_axis(&self) -> bool {
        self.keybind_set == KeybindSet::ThreeKeyStrict && self.current_turn.side.is_some()
    }

    fn handle_prefix_keys(&mut self, c: char) -> bool {
        if c == self.prefs.global_keys.keybind_mode {
            self.flush_modes();
            self.keybind_set = self.keybind_set.next(self.puzzle.n);
            self.message = Some(format!("set keybinds to {}", self.keybind_set.name()));
            return true;
        }
        if c == self.prefs.global_keys.axis_mode {
            if self.puzzle.d > 6 {
                self.message = Some("not enough room for side keybinds".to_string());
            } else {
                self.flush_modes();
                self.keybind_axial = self.keybind_axial.next();
                self.message =
                    Some(format!("set axis mode to {}", self.keybind_axial.name()));
            }
            return true;
        }
        if c == self.prefs.global_keys.undo {
            self.flush_modes();
            let undid = self.undo_history.pop();
            match undid {
                None => self.message = Some("nothing to undo".to_string()),
                Some(undid) => {
                    self.puzzle.turn(undid.inverse());
                    self.redo_history.push(undid);
                    self.rev_stack_adjust();
                }
            }
            return true;
        }
        if c == self.prefs.global_keys.redo {
            self.flush_modes();
            let redid = self.redo_history.pop();
            match redid {
                None => self.message = Some("nothing to redo".to_string()),
                Some(redid) => {
                    self.puzzle.turn(redid.clone());
                    self.undo_history.push(redid);
                    self.rev_stack_adjust();
                }
            }
            return true;
        }
        if c == self.prefs.global_keys.next_filter {
            if self.filters.is_empty() {
                self.message = Some("no filters loaded".to_string());
            } else {
                self.flush_modes();
                self.filter_ind += 1;
                self.use_live_filter = false;
                self.message = Some("next filter".to_string());
            }
            return true;
        }
        if c == self.prefs.global_keys.prev_filter {
            if self.filters.is_empty() {
                self.message = Some("no filters loaded".to_string());
            } else {
                self.flush_modes();
                self.filter_ind -= 1;
                self.use_live_filter = false;
                self.message = Some("previous filter".to_string());
            }
            return true;
        }
        if let Some(s) = self.prefs.global_keys.layers.iter().position(|ch| ch == &c) {
            if s as i16 >= self.puzzle.n {
                return true;
            }
            self.flush_modes();
            self.current_turn.layer_char = Some(c);
            self.current_turn.layer = Some(TurnLayer::Layer(s as i16));
            return true;
        }
        if self.current_turn.layer != Some(TurnLayer::WholePuzzle)
            && !self.awaiting_side_as_axis()
            && let Some(kp) = self.get_side(c)
            && !(self.current_turn.side.is_some() && self.get_axis_key(c).is_some())
        {
            if ax(kp.axis) as u16 >= self.puzzle.d {
                return true;
            }
            if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                self.flush_modes();
            }
            self.current_turn.side = Some(kp);
            return true;
        }
        if c == self.prefs.global_keys.rotate {
            self.flush_modes();
            self.current_turn.layer_char = Some(c);
            self.current_turn.layer = Some(TurnLayer::WholePuzzle);
            return true;
        }
        false
    }

    fn try_three_key(&mut self, c: char) {
        let strict = self.keybind_set == KeybindSet::ThreeKeyStrict;
        let axis = if strict {
            self.get_side(c)
        } else {
            self.get_axis_key(c)
        };
        let Some(kp) = axis else { return };

        if !(self.current_turn.side.is_some()
            || self.current_turn.layer == Some(TurnLayer::WholePuzzle))
        {
            return;
        }
        if ax(kp.axis) as u16 >= self.puzzle.d {
            return;
        }

        let side_axis = if let Some(side_kp) = self.current_turn.side {
            side_kp.axis
        } else if self.current_turn.layer == Some(TurnLayer::WholePuzzle) {
            0
        } else {
            return;
        };

        if self.current_turn.from.is_none() {
            self.last_turn_keys.clear();
        }

        if let Some(from_kp) = self.current_turn.from {
            let (from_norm, to_norm) =
                if self.current_turn.layer == Some(TurnLayer::WholePuzzle) {
                    let f = ax(from_kp.axis);
                    let t = ax(kp.axis);
                    if (from_kp.axis < 0) != (kp.axis < 0) {
                        (t, f)
                    } else {
                        (f, t)
                    }
                } else {
                    (from_kp.axis, kp.axis)
                };
            // perform_turn must be called BEFORE clearing any state
            if self.perform_turn(side_axis, from_norm, to_norm).is_some() {
                let mut keys = self.current_turn.current_keys();
                keys.push(kp.ch);
                self.last_turn_keys = keys;
            } else {
                self.alert = self.prefs.alert_frames * 4 - 1;
                self.last_turn_keys.clear();
            }
            self.current_turn.from = None;
            if strict {
                self.current_turn.clear();
            }
        } else {
            self.current_turn.from = Some(kp);
        }
    }

    fn try_fixed_3d(&mut self, c: char) {
        let (axis, flip) =
            if let Some(s) = self.prefs.axes.iter().position(|ax| ax.pos.keys.side == c) {
                (s as i16, true)
            } else if let Some(s) =
                self.prefs.axes.iter().position(|ax| ax.neg.keys.side == c)
            {
                (!(s as i16), true)
            } else {
                return;
            };
        if ax(axis) as u16 >= self.puzzle.d {
            return;
        }
        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
            self.flush_modes();
        }
        self.current_turn.side = Some(KeyPress { ch: c, axis });

        let (from, to) = if flip {
            if axis < 0 {
                ((!axis + 1) % 3, (!axis + 2) % 3)
            } else {
                ((axis + 2) % 3, (axis + 1) % 3)
            }
        } else if axis < 0 {
            ((!axis + 2) % 3, (!axis + 1) % 3)
        } else {
            ((axis + 1) % 3, (axis + 2) % 3)
        };
        if self.perform_turn(axis, from, to).is_some() {
            self.last_turn_keys = self.current_turn.current_keys();
        } else {
            self.alert = self.prefs.alert_frames * 4 - 1;
            self.last_turn_keys.clear();
        }
    }

    fn try_fixed_key(&mut self, c: char) {
        let Some(kp) = self.get_axis_key(c) else { return };
        if ax(kp.axis) as u16 >= self.puzzle.d {
            return;
        }

        let whole_puzzle = self.current_turn.layer == Some(TurnLayer::WholePuzzle);
        let required_fixed = if whole_puzzle {
            self.puzzle.d as usize - 2
        } else {
            self.puzzle.d as usize - 3
        };
        if !whole_puzzle && self.current_turn.side.is_none() {
            return;
        }

        if self.current_turn.fixed.is_empty() {
            self.last_turn_keys.clear();
        }
        self.current_turn.fixed.push(kp);
        if self.current_turn.fixed.len() != required_fixed {
            return;
        }

        let mut sign = true;
        let mut axes: Vec<i16> = if let Some(side_kp) = &self.current_turn.side {
            vec![side_kp.axis]
        } else {
            vec![]
        };
        axes.extend(self.current_turn.fixed.iter().map(|kp| kp.axis));
        for axis in &mut axes {
            if *axis < 0 {
                sign = !sign;
                *axis = !*axis;
            }
        }
        for axis in 0..self.puzzle.d as i16 {
            if !axes.contains(&axis) {
                axes.push(axis);
            }
        }
        if axes.len() > self.puzzle.d as usize {
            self.alert = self.prefs.alert_frames * 4 - 1;
            self.current_turn.fixed.clear();
            return;
        }
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
        let side_axis = self.current_turn.side.as_ref().map(|kp| kp.axis).unwrap_or(0);
        // Capture canonical string BEFORE clearing fixed
        if self.perform_turn(side_axis, from, to).is_some() {
            self.last_turn_keys = self.current_turn.current_keys();
        } else {
            self.alert = self.prefs.alert_frames * 4 - 1;
            self.last_turn_keys.clear();
        }
        self.current_turn.fixed.clear();
    }

    fn get_side(&self, c: char) -> Option<KeyPress> {
        self.prefs
            .axes
            .iter()
            .position(|ax| ax.pos.keys.select == c)
            .map(|s| KeyPress { ch: c, axis: s as i16 })
            .or_else(|| {
                self.prefs
                    .axes
                    .iter()
                    .position(|ax| ax.neg.keys.select == c)
                    .map(|s| KeyPress { ch: c, axis: !(s as i16) })
            })
    }

    pub fn make_layout(&self, semi_compact: bool, compact: bool, vertical: bool) -> Layout {
        let mut layout = Layout::make_layout(self.puzzle.n, self.puzzle.d, semi_compact, compact, vertical).move_right(1);
        layout.width += 1; // reserve rightmost column for brackets
        layout
    }

    pub fn process_key(&mut self, c: char) {
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
                    self.rev_stack.clear();
                } else if ch == self.prefs.global_keys.reset {
                    self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                    self.message = Some("puzzle reset".to_string());
                    self.scramble = self.puzzle.clone();
                    self.undo_history = vec![];
                    self.redo_history = vec![];
                    self.rev_stack.clear();
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
        } else {
            match self.mode {
                AppMode::Turn => {
                    let consumed = self.handle_prefix_keys(c);
                    if consumed {
                        self.last_turn_keys.clear();
                        return;
                    }
                    match self.keybind_set {
                        KeybindSet::ThreeKey | KeybindSet::ThreeKeyStrict => {
                            self.try_three_key(c);
                        }
                        KeybindSet::FixedKey if self.puzzle.d == 3 => {
                            self.try_fixed_3d(c);
                        }
                        KeybindSet::FixedKey => {
                            self.try_fixed_key(c);
                        }
                    }
                }

                AppMode::LiveFilter => {
                    if c == '+' || c == '!' {
                        self.live_filter_string.push(c);
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
                    } else if self
                        .prefs
                        .axes
                        .iter()
                        .any(|ax| ax.pos.name == c || ax.neg.name == c)
                    {
                        self.live_filter_string.push(c);
                    } else if let Some(ind) = filters::DIGITS.chars().position(|ch| c == ch) {
                        if ind <= self.puzzle.d as usize {
                            self.live_filter_string.push(c);
                        }
                    } else if c == BACKSPACE_CODE {
                        self.live_filter_string.pop();
                    }

                    let filter_result: Result<Filter, _> =
                        Filter::parse(&self.live_filter_string, &self.prefs);
                    if let Ok(filter) = &filter_result {
                        self.live_filter_pending = filter.clone();
                    }

                    if c == '\n' {
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

    fn get_axis_key(&self, c: char) -> Option<KeyPress> {
        if self.keybind_set == KeybindSet::ThreeKeyStrict {
            return None;
        }

        match self.keybind_axial {
            KeybindAxial::Axial => self
                .prefs
                .axes
                .iter()
                .position(|ax| ax.axis_key == c)
                .map(|s| KeyPress { ch: c, axis: s as i16 }),
            KeybindAxial::Side => self.prefs.axes.iter().enumerate().find_map(|(s, ax)| {
                (ax.pos.keys.side == c)
                    .then_some(KeyPress { ch: c, axis: s as i16 })
                    .or_else(|| {
                        (ax.neg.keys.side == c).then_some(KeyPress { ch: c, axis: !(s as i16) })
                    })
            }),
        }
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

        let turn_clone = turn.clone();
        let turn_out = self.puzzle.turn(turn);

        if turn_out.is_some() {
            self.undo_history.push(turn_clone);
            if self.puzzle.is_solved() {
                self.message = Some("solved!".to_string());
            }
        }

        turn_out
    }

    fn get_message(&self) -> String {
        if let Some(message) = &self.message {
            return message.to_string();
        }
        match self.mode {
            AppMode::Turn => {
                if !self.last_turn_keys.is_empty() {
                    self.last_turn_keys.clone()
                } else {
                    self.current_turn.current_keys()
                }
            }
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

    fn rev_start(&mut self) {
        self.rev_stack.push(RevEntry {
            start: self.undo_history.len(),
            end: None,
        });
    }

    fn rev_stop(&mut self) {
        if let Some(top) = self.rev_stack.last() {
            if top.end.is_some() || self.undo_history.len() <= top.start {
                self.rev_stack.pop();
            } else {
                self.rev_stack.last_mut().unwrap().end = Some(self.undo_history.len());
            }
        }
    }

    fn apply_reverse(&mut self, from: usize, to: usize) {
        let turns: Vec<Turn> = self.undo_history[from..to]
            .iter()
            .rev()
            .cloned()
            .collect();
        for turn in turns {
            let inverse = turn.inverse();
            self.puzzle.turn(inverse.clone());
            self.undo_history.push(inverse);
        }
    }

    fn rev_unwind(&mut self) {
        if let Some(entry) = self.rev_stack.pop() {
            if let Some(r) = entry.end {
                if self.undo_history.len() >= r {
                    self.apply_reverse(entry.start, r);
                }
            }
        }
    }

    fn rev_commutator(&mut self) {
        if let Some(entry) = self.rev_stack.pop() {
            if let Some(r) = entry.end {
                let p = self.undo_history.len();
                if p >= r {
                    self.apply_reverse(entry.start, r);
                    self.apply_reverse(r, p);
                }
            }
        }
    }

    fn rev_stack_display(&self) -> String {
        let mut s = String::new();
        let mut pos = 0usize;
        for entry in &self.rev_stack {
            if entry.start > pos {
                s.push_str(&(entry.start - pos).to_string());
            }
            s.push('[');
            if let Some(end) = entry.end {
                s.push_str(&(end - entry.start).to_string());
                s.push(']');
                pos = end;
            } else {
                pos = entry.start;
            }
        }
        if pos < self.undo_history.len() {
            s.push_str(&(self.undo_history.len() - pos).to_string());
        }
        s
    }

    fn rev_stack_adjust(&mut self) {
        let len = self.undo_history.len();
        self.rev_stack.retain_mut(|entry| {
            if entry.start > len {
                if entry.end.is_none() {
                    entry.start = len;
                } else {
                    return false;
                }
            }
            if let Some(ref mut end) = entry.end {
                if *end > len {
                    *end = len;
                }
                if *end == entry.start {
                    return false;
                }
            }
            true
        });
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
    let x = x.max(1);
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
    let x = x.max(1);
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
        let _ = terminal::disable_raw_mode();
        let _ = stdout.execute(terminal::LeaveAlternateScreen);
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

    /// Display in semi-compact mode (1-char gaps between layers)
    #[arg(short = 'c')]
    semi_compact: bool,

    /// Display in compact mode (no gaps between layers)
    #[arg(long)]
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

    /// Preferences file
    #[arg(short, long)]
    prefs: Option<PathBuf>,
}

pub fn main_inner() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let prefs: Prefs = if let Some(path) = args.prefs {
        let file = File::open(path)?;
        serde_json::from_reader(BufReader::new(file))?
    } else {
        Prefs::load_default()?
    };
    prefs.validate()?;

    let mut state;
    if let Some(log_file) = args.log {
        let file = File::open(log_file)?;
        let reader = BufReader::new(file);
        let app_log = serde_json::from_reader(reader).map_err(std::io::Error::other)?;
        state = AppState::from_app_log(app_log, prefs);
    } else {
        state = AppState::new(args.n, args.d, prefs)?;
    }

    if let Some(path) = args.filters {
        let filters_str = std::fs::read_to_string(path).expect("Invalid filter file");
        state.filters = filters_str
            .lines()
            .map(|l| Filter::parse(&l, &state.prefs).unwrap())
            .collect();
    }

    let layout = state.make_layout(args.semi_compact, args.compact, args.vertical);
    //println!("{:?}", layout.keybind_hints);
    //return Ok(());

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(cursor::Hide)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;

    let stdout_manager = StdoutManager;

    ctrlc::set_handler(move || {
        if CTRL_C_PRESSED.swap(true, Ordering::SeqCst) {
            std::process::exit(0); // second Ctrl+C — terminate immediately
        }
    })?;

    const SCROLL_STEP_WHEEL: i16 = 3;

    let (mut term_w, mut term_h) = terminal::size()?;
    let mut scroll_max_x = layout.width.saturating_sub(term_w) as i16;
    let mut scroll_max_y = layout.height.saturating_sub(term_h.saturating_sub(2)) as i16;
    let mut scroll_x: i16 = 0;
    let mut scroll_y: i16 = 0;
    let mut prev_mouse_pos: Option<(u16, u16)> = None;
    let mut dragged = false;
    let mut last_empty_click: Option<Instant> = None;
    const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(500);

    'event: loop {
        let previous_message = state.get_message();
        let previous_rev_stack = state.rev_stack_display();
        let previous_hovered = state.hovered;
        let previous_clicked_stickers = state.clicked_stickers();
        let mut just_resized = false;

        let frame_begin = Instant::now();
        let scroll_before = (scroll_x, scroll_y);

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
                    KeyCode::F(1) => state.rev_start(),
                    KeyCode::F(2) => state.rev_stop(),
                    KeyCode::F(3) => state.rev_unwind(),
                    KeyCode::F(4) => state.rev_commutator(),
                    KeyCode::Char(c) => {
                        state.process_key(c);
                    }
                    KeyCode::Tab => {
                        state.process_key('\t');
                    }
                    KeyCode::Esc => {
                        state.process_key(ESCAPE_CODE);
                    }
                    KeyCode::Enter => {
                        state.process_key('\n');
                    }
                    KeyCode::Backspace => {
                        state.process_key(BACKSPACE_CODE);
                    }
                    KeyCode::Up => {
                        let step = (term_h.saturating_sub(2) as i16 * 3 / 4).max(1);
                        scroll_y = (scroll_y - step).max(0);
                    }
                    KeyCode::Down => {
                        let step = (term_h.saturating_sub(2) as i16 * 3 / 4).max(1);
                        scroll_y = (scroll_y + step).min(scroll_max_y);
                    }
                    KeyCode::Left => {
                        let step = (term_w as i16 * 3 / 4).max(1);
                        scroll_x = (scroll_x - step).max(0);
                    }
                    KeyCode::Right => {
                        let step = (term_w as i16 * 3 / 4).max(1);
                        scroll_x = (scroll_x + step).min(scroll_max_x);
                    }
                    _ => (),
                },
                Event::Mouse(MouseEvent {
                    kind,
                    column,
                    row,
                    modifiers,
                    ..
                }) => {
                    match kind {
                        MouseEventKind::ScrollUp => {
                            if modifiers.contains(KeyModifiers::SHIFT) {
                                scroll_x = (scroll_x - SCROLL_STEP_WHEEL).max(0);
                            } else {
                                scroll_y = (scroll_y - SCROLL_STEP_WHEEL).max(0);
                            }
                            continue;
                        }
                        MouseEventKind::ScrollDown => {
                            if modifiers.contains(KeyModifiers::SHIFT) {
                                scroll_x = (scroll_x + SCROLL_STEP_WHEEL).min(scroll_max_x);
                            } else {
                                scroll_y = (scroll_y + SCROLL_STEP_WHEEL).min(scroll_max_y);
                            }
                            continue;
                        }
                        MouseEventKind::ScrollLeft => {
                            scroll_x = (scroll_x - SCROLL_STEP_WHEEL).max(0);
                            continue;
                        }
                        MouseEventKind::ScrollRight => {
                            scroll_x = (scroll_x + SCROLL_STEP_WHEEL).min(scroll_max_x);
                            continue;
                        }
                        MouseEventKind::Drag(_) => {
                            if let Some((prev_col, prev_row)) = prev_mouse_pos {
                                let dx = prev_col as i16 - column as i16;
                                let dy = prev_row as i16 - row as i16;
                                scroll_x = (scroll_x + dx).clamp(0, scroll_max_x);
                                scroll_y = (scroll_y + dy).clamp(0, scroll_max_y);
                            }
                            prev_mouse_pos = Some((column, row));
                            dragged = true;
                            continue;
                        }
                        MouseEventKind::Up(_) => {
                            prev_mouse_pos = None;
                            if std::mem::replace(&mut dragged, false) {
                                continue;
                            }
                        }
                        MouseEventKind::Down(_) => {
                            prev_mouse_pos = Some((column, row));
                            dragged = false;
                            continue;
                        }
                        _ => {}
                    }
                    let key = (column as i16 + scroll_x, row as i16 + scroll_y);
                    let sticker = layout.points.get(&key);
                    if let Some(sticker) = sticker {
                        if let MouseEventKind::Up(_) = kind {
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
                    } else if let MouseEventKind::Up(_) = kind {
                        if let Some(prev) = last_empty_click
                            && prev.elapsed() < DOUBLE_CLICK_THRESHOLD
                        {
                            state.clicked.clear();
                            last_empty_click = None;
                        } else {
                            last_empty_click = Some(Instant::now());
                        }
                    }
                    if let MouseEventKind::Moved = kind {
                        state.hovered = sticker.map(|_| key);
                    }
                }
                Event::Resize(_, _) => {
                    let (new_w, new_h) = terminal::size()?;
                    term_w = new_w;
                    term_h = new_h;
                    scroll_max_x = layout.width.saturating_sub(term_w) as i16;
                    scroll_max_y = layout.height.saturating_sub(term_h.saturating_sub(2)) as i16;
                    scroll_x = scroll_x.min(scroll_max_x);
                    scroll_y = scroll_y.min(scroll_max_y);
                    stdout.execute(terminal::Clear(terminal::ClearType::All))?;
                    just_resized = true;
                }
                _ => (),
            }
        }

        if CTRL_C_PRESSED.load(Ordering::SeqCst) {
            break 'event;
        }

        let scrolled = (scroll_x, scroll_y) != scroll_before;
        if scrolled {
            stdout.execute(terminal::Clear(terminal::ClearType::All))?;
        }

        let message = state.get_message();
        let rev_stack_display = state.rev_stack_display();

        if just_resized {
            stdout
                .queue(cursor::MoveTo(0, term_h.saturating_sub(2)))?
                .queue(terminal::Clear(terminal::ClearType::All))?
                .flush()?;
        }
        if previous_rev_stack != rev_stack_display || scrolled || just_resized {
            stdout
                .queue(cursor::MoveTo(0, term_h.saturating_sub(2)))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?;
            if !rev_stack_display.is_empty() {
                stdout.queue(style::Print(&format!(
                    "RevStack: {}",
                    rev_stack_display
                )))?;
            }
        }
        if previous_message != message || scrolled || just_resized {
            stdout
                .queue(cursor::MoveTo(0, term_h.saturating_sub(1)))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .queue(style::Print(&message))?;
        }

        if let Some((x, y)) = previous_hovered {
            let sx = x - scroll_x;
            let sy = y - scroll_y;
            if sx >= 1 && (sx as u16) < term_w && sy >= 0 && (sy as u16) < term_h.saturating_sub(2) {
                erase_brackets(&mut stdout, sx, sy)?;
            }
        }

        if let Some((x, y)) = state.hovered {
            let sx = x - scroll_x;
            let sy = y - scroll_y;
            if sx >= 1 && (sx as u16) < term_w && sy >= 0 && (sy as u16) < term_h.saturating_sub(2) {
                draw_brackets(
                    &mut stdout,
                    sx,
                    sy,
                    ClickedStyle::Hovered,
                    &state.prefs,
                )?;
            }
        }

        let clicked_stickers = state.clicked_stickers();
        let mut erase_locs = HashSet::new();
        let mut clicked_locs: HashMap<_, _> = CLICKED_STYLES
            .into_iter()
            .map(|s| (s, HashSet::new()))
            .collect();

        for ((x, y), pos) in &layout.points {
            let screen_x = *x - scroll_x;
            let screen_y = *y - scroll_y;
            if screen_x < 0
                || screen_x >= term_w as i16
                || screen_y < 0
                || screen_y >= term_h.saturating_sub(2) as i16
            {
                continue;
            }
            // in this loop we are more efficient by not flushing the buffer.
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

            let in_filter = filter.matches_stickers(&state.puzzle.stickers(pos));

            if pos.iter().any(|x| x.abs() == state.puzzle.n) {
                let side = state.puzzle.stickers[pos];
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
                    .queue(cursor::MoveTo(screen_x as u16, screen_y as u16))?
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
                    .queue(cursor::MoveTo(screen_x as u16, screen_y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }

            if previous_clicked_stickers.get(pos) != clicked_stickers.get(pos) {
                erase_locs.insert((screen_x, screen_y));
            }

            if let Some(style) = clicked_stickers.get(pos) {
                clicked_locs
                    .get_mut(style)
                    .expect("contains")
                    .insert((screen_x, screen_y));
            }
        }

        for (x, y) in erase_locs {
            if x >= 1 && (x as u16) < term_w && y >= 0 && (y as u16) < term_h.saturating_sub(2) {
                erase_brackets(&mut stdout, x, y)?;
            }
        }

        for style in CLICKED_STYLES {
            for (x, y) in clicked_locs.get(style).expect("contains") {
                if *x >= 1 && (*x as u16) < term_w && *y >= 0 && (*y as u16) < term_h.saturating_sub(2) {
                    draw_brackets(&mut stdout, *x, *y, *style, &state.prefs)?;
                }
            }
        }

        for ((x, y), side) in &layout.keybind_hints {
            let screen_x = *x - scroll_x;
            let screen_y = *y - scroll_y;
            if screen_x < 0
                || screen_x >= term_w as i16
                || screen_y < 0
                || screen_y >= term_h.saturating_sub(2) as i16
            {
                continue;
            }
            // in this loop we are more efficient by not flushing the buffer.
            let ch;
            let color;
            if let Some(side) = side {
                ch = if (state.current_turn.side.is_none()
                        && state.current_turn.layer != Some(TurnLayer::WholePuzzle))
                    || (state.keybind_set == KeybindSet::FixedKey
                        && state.puzzle.d == 3
                        && state.current_turn.layer != Some(TurnLayer::WholePuzzle))
                    || state.keybind_set == KeybindSet::ThreeKeyStrict
                {
                    if *side >= 0 {
                        state.prefs.axes[*side as usize].pos.keys.select
                    } else {
                        state.prefs.axes[(!side) as usize].neg.keys.select
                    }
                } else {
                    match state.keybind_axial {
                        KeybindAxial::Axial => {
                            if *side >= 0 {
                                state.prefs.axes[*side as usize].axis_key
                            } else {
                                '·'
                            }
                        }
                        KeybindAxial::Side => {
                            if *side >= 0 {
                                state.prefs.axes[*side as usize].pos.keys.side
                            } else {
                                state.prefs.axes[(!side) as usize].neg.keys.side
                            }
                        }
                    }
                };
                color = state.prefs.global_colors.piece;

                stdout
                    .queue(cursor::MoveTo(screen_x as u16, screen_y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }
        }

        stdout.queue(cursor::MoveTo(0, term_h.saturating_sub(1)))?.flush()?;

        if state.alert > 0 {
            state.alert -= 1;
        }

        let frame_end = Instant::now();
        let frame = frame_end - frame_begin;
        if frame < FRAME_LENGTH {
            std::thread::sleep(FRAME_LENGTH - frame);
        }
        //state.puzzle.turn(0, 2, 2, 1); // R
    }

    drop(stdout_manager);
    Ok(())
}
