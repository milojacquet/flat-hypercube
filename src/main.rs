use clap::Parser;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use filters::Filter;
use layout::Layout;
use puzzle::{ax, Puzzle, PuzzleTurn, SideTurn, Turn};
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::{self, Write};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

mod filters;
mod layout;
mod puzzle;

const fn hex(hex: u32) -> Color {
    Color::Rgb {
        r: ((hex >> 16) & 0xff) as u8,
        g: ((hex >> 8) & 0xff) as u8,
        b: ((hex >> 0) & 0xff) as u8,
    }
}

const POS_NAMES: &[char] = &['R', 'U', 'F', 'O', 'A', 'Γ', 'Θ', 'Ξ', 'Σ', 'Ψ'];
const NEG_NAMES: &[char] = &['L', 'D', 'B', 'I', 'P', 'Δ', 'Λ', 'Π', 'Φ', 'Ω'];
const POS_KEYS: &[char] = &['f', 'e', 'r', 't', 'v', 'y', 'n', 'q', ',', '/'];
const NEG_KEYS: &[char] = &['s', 'd', 'w', 'g', 'c', 'h', 'b', 'a', 'm', '.'];
const POS_KEYS_RIGHT: &[char] = &['l', 'i', 'j', '.', 'p', '['];
const NEG_KEYS_RIGHT: &[char] = &['u', ',', 'o', 'k', 'l', ';'];
const AXIS_KEYS: &[char] = &['k', 'j', 'l', 'i', 'u', 'o', 'p', ';', '[', '\''];
const LAYER_KEYS: &[char] = &['1', '2', '3', '4', '5', '6', '7', '8', '9'];
const ESCAPE_CODE: char = '⎋';
const BACKSPACE_CODE: char = '⌫';
const ROT_KEY: char = 'x';
const SCRAMBLE_KEY: char = '=';
const RESET_KEY: char = '-';
const DAMAGE_REPEAT: u8 = 5;
const KEYBIND_KEY: char = '\\';
const KEYBIND_AXIAL_KEY: char = '|';
const UNDO_KEY: char = 'z';
const REDO_KEY: char = 'Z';
const NEXT_FILTER_KEY: char = 'K';
const PREV_FILTER_KEY: char = 'J';
const LIVE_FILTER_MODE_KEY: char = 'F';
const RESET_MODE_KEY: char = ESCAPE_CODE;
const SAVE_KEY: char = 'S';
const MAX_DIM: u16 = 10;
const MAX_LAYERS: i16 = 19;

const POS_COLORS: &[Color] = &[
    hex(0xff0000),
    hex(0xffffff),
    hex(0x00ff00),
    hex(0xff00ff),
    hex(0x0aaa85),
    hex(0x774811),
    hex(0xf49fef),
    hex(0xb29867),
    hex(0x9cf542),
    hex(0x078517),
];
const NEG_COLORS: &[Color] = &[
    hex(0xff8000),
    hex(0xffff00),
    hex(0x0080ff),
    hex(0x8f10ea),
    hex(0x7daa0a),
    hex(0x6d4564),
    hex(0xd4a94e),
    hex(0xb27967),
    hex(0x42d4f5),
    hex(0x2f2fbd),
];
const PIECE_COLOR: Color = hex(0x808080);
const FILTERED_COLOR: Color = hex(0x505050);
const ALERT_COLOR: Color = hex(0xd86c6c);
const FRAME_LENGTH: Duration = Duration::from_millis(1000 / 30);
const ALERT_FRAMES: u8 = 4;

#[derive(PartialEq)]
enum TurnLayer {
    Layer(i16),
    WholePuzzle,
}

#[derive(Default)]
struct TurnBuild {
    layer: Option<TurnLayer>,
    side: Option<i16>,
    from: Option<i16>,
    fixed: Vec<i16>,
}

enum KeybindAxial {
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

#[derive(PartialEq)]
enum KeybindSet {
    ThreeKey, // MC7D, works in d dimensions, depends on axial flag
    FixedKey, // works in d dimensions, requires d-2 keypresses, depends on axial flag
              // has addition inversion keys in 3d
              //XyzKey, // HSC, 4d only
}

impl KeybindSet {
    fn valid(&self, n: i16) -> bool {
        match self {
            Self::ThreeKey => true,
            Self::FixedKey => n >= 3,
            //Self::XyzKey => n == 4,
        }
    }

    fn next(&self, n: i16) -> Self {
        let next = match self {
            Self::ThreeKey => Self::FixedKey,
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
            Self::FixedKey => "fixed-key".to_string(),
            //Self::XyzKey => "xyz".to_string(),
        }
    }
}

#[derive(Default)]
enum AppMode {
    #[default]
    Turn,
    LiveFilter,
}

struct AppState {
    puzzle: Puzzle,
    scramble: Puzzle,
    mode: AppMode,
    current_keys: String,
    current_turn: TurnBuild,
    alert: u8,
    damage_counter: Option<(char, u8)>,
    rng: ThreadRng,
    keybind_set: KeybindSet,
    keybind_axial: KeybindAxial,
    message: Option<String>,
    undo_history: Vec<Turn>,
    redo_history: Vec<Turn>,
    filters: Vec<Filter>,
    filter_ind: usize,
    use_live_filter: bool,
    live_filter_string: String,
    live_filter_pending: Filter,
    live_filter: Filter,
    filename: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct AppLog {
    scramble: Puzzle,
    moves: Vec<Turn>,
}

impl AppState {
    fn new(n: i16, d: u16) -> Self {
        Self {
            puzzle: Puzzle::make_solved(n, d),
            scramble: Puzzle::make_solved(n, d),
            mode: Default::default(),
            current_keys: "".to_string(),
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
            filename: Self::new_filename(),
        }
    }

    fn to_app_log(&self) -> AppLog {
        AppLog {
            scramble: self.scramble.clone(),
            moves: self.undo_history.clone(),
        }
    }

    fn from_app_log(app_log: AppLog) -> Self {
        let mut state = AppState::new(app_log.scramble.n, app_log.scramble.d);
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
        self.current_keys = "".to_string();
        self.current_turn = Default::default();
        self.live_filter_string = Default::default();
    }

    fn process_key(&mut self, c: char, _mods: KeyModifiers) {
        self.message = None;
        if c == SCRAMBLE_KEY || c == RESET_KEY {
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

        if let Some((ch, DAMAGE_REPEAT)) = self.damage_counter {
            self.flush_modes();
            if ch == SCRAMBLE_KEY && self.puzzle.d >= 3 {
                self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                self.puzzle.scramble(&mut self.rng);
                self.message = Some("scrambled with 5000 turns".to_string());
                self.scramble = self.puzzle.clone();
                self.undo_history = vec![];
                self.redo_history = vec![];
            } else if ch == RESET_KEY {
                self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                self.message = Some("puzzle reset".to_string());
                self.scramble = self.puzzle.clone();
                self.undo_history = vec![];
                self.redo_history = vec![];
            }
            self.damage_counter = None;
        } else if c == RESET_MODE_KEY {
            self.mode = Default::default();
            self.flush_modes();
            self.message = None;
        } else if c == LIVE_FILTER_MODE_KEY && !matches!(self.mode, AppMode::LiveFilter) {
            self.mode = AppMode::LiveFilter;
        } else if c == SAVE_KEY {
            match self.save() {
                Ok(()) => self.message = Some(format!("saved to {}", self.filename.display())),
                //Err(err) => self.message = Some(format!("could not save: {}", err)),
                Err(_err) => self.message = Some("could not save".to_string()),
            }
        } else {
            match self.mode {
                AppMode::Turn => {
                    let mut just_pressed_side = false;

                    if c == KEYBIND_KEY {
                        self.flush_modes();
                        self.keybind_set = self.keybind_set.next(self.puzzle.n);
                        self.message = Some(format!("set keybinds to {}", self.keybind_set.name()))
                    } else if c == KEYBIND_AXIAL_KEY {
                        if self.puzzle.d > 6 {
                            self.message = Some("not enough room for side keybinds".to_string());
                        } else {
                            self.flush_modes();
                            self.keybind_axial = self.keybind_axial.next();
                            self.message =
                                Some(format!("set axis mode to {}", self.keybind_axial.name()))
                        }
                    } else if c == UNDO_KEY {
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
                    } else if c == REDO_KEY {
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
                    } else if c == NEXT_FILTER_KEY {
                        if self.filters.is_empty() {
                            self.message = Some("no filters loaded".to_string());
                        } else {
                            self.flush_modes();
                            self.filter_ind += 1;
                            self.use_live_filter = false;
                            self.message = Some("next filter".to_string());
                        }
                    } else if c == PREV_FILTER_KEY {
                        if self.filters.is_empty() {
                            self.message = Some("no filters loaded".to_string());
                        } else {
                            self.flush_modes();
                            self.filter_ind -= 1;
                            self.use_live_filter = false;
                            self.message = Some("previous filter".to_string());
                        }
                    } else if let Some(s) = LAYER_KEYS.iter().position(|ch| ch == &c) {
                        if s as i16 >= self.puzzle.n {
                            return;
                        }
                        self.flush_modes();
                        self.current_keys.push(c);
                        self.current_turn.layer = Some(TurnLayer::Layer(s as i16));
                    } else if let Some(s) = POS_KEYS.iter().position(|ch| ch == &c) {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                            self.flush_modes();
                        }
                        self.current_keys.push(c);
                        self.current_turn.side = Some(s as i16);
                        just_pressed_side = true;
                    } else if let Some(s) = NEG_KEYS.iter().position(|ch| ch == &c) {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                            self.flush_modes();
                        }
                        self.current_keys.push(c);
                        self.current_turn.side = Some(!(s as i16));
                        just_pressed_side = true;
                    } else if c == ROT_KEY {
                        if self.keybind_set == KeybindSet::ThreeKey {
                            self.flush_modes();
                            just_pressed_side = true;
                        }
                        self.current_keys.push(c);
                        self.current_turn.layer = Some(TurnLayer::WholePuzzle);
                    }

                    match self.keybind_set {
                        KeybindSet::ThreeKey => {
                            let axis = self.get_axis_key(c);

                            if let (Some(s), true) = (
                                axis,
                                self.current_turn.side.is_some()
                                    || self.current_turn.layer == Some(TurnLayer::WholePuzzle),
                            ) {
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
                                            self.alert = ALERT_FRAMES * 4 - 1;
                                            self.current_keys = self.current_keys
                                                [..self.current_keys.len() - 2]
                                                .to_string();
                                        }
                                        self.current_turn.from = None;
                                    } else {
                                        self.current_turn.from = Some(s);
                                    }
                                }
                            }
                        }
                        KeybindSet::FixedKey if self.puzzle.d == 3 => {
                            let flip;
                            if let Some(s) = POS_KEYS_RIGHT.iter().position(|ch| ch == &c) {
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
                            } else if let Some(s) = NEG_KEYS_RIGHT.iter().position(|ch| ch == &c) {
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
                                            self.alert = ALERT_FRAMES * 4 - 1;
                                            self.current_keys =
                                                self.current_keys[..self.current_keys.len()
                                                    - self.current_turn.fixed.len()]
                                                    .to_string();
                                        }
                                        self.current_turn.fixed = vec![];
                                    }
                                }
                            }
                        } //_ => todo!(),
                    }
                }

                AppMode::LiveFilter => {
                    if c == '+' || c == '!' {
                        self.live_filter_string.push(c);
                    } else if let Some(s) = POS_KEYS.iter().position(|ch| ch == &c) {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        self.live_filter_string.push(POS_NAMES[s]);
                    } else if let Some(s) = NEG_KEYS.iter().position(|ch| ch == &c) {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        self.live_filter_string.push(NEG_NAMES[s]);
                    } else if POS_NAMES.iter().any(|ch| ch == &c)
                        || NEG_NAMES.iter().any(|ch| ch == &c)
                    {
                        self.live_filter_string.push(c);
                    } else if c == BACKSPACE_CODE {
                        self.live_filter_string.pop();
                    }

                    let filter_result: Result<Filter, _> = self.live_filter_string.parse();
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

    fn get_axis_key(&self, c: char) -> Option<i16> {
        match self.keybind_axial {
            KeybindAxial::Axial => AXIS_KEYS.iter().position(|ch| ch == &c),
            KeybindAxial::Side => POS_KEYS_RIGHT
                .iter()
                .position(|ch| ch == &c)
                .or_else(|| NEG_KEYS_RIGHT.iter().position(|ch| ch == &c).map(|s| !s)),
        }
        .map(|s| s as i16)
    }

    fn perform_turn(&mut self, side: i16, from: i16, to: i16) -> Option<()> {
        let turn = match self.current_turn.layer {
            Some(TurnLayer::WholePuzzle) => Turn::Puzzle(PuzzleTurn { from, to }),
            _ => {
                let mut layer_min;
                let mut layer_max;
                match self.current_turn.layer {
                    None => {
                        layer_min = self.puzzle.n - 1;
                        layer_max = self.puzzle.n - 1;
                    }
                    Some(TurnLayer::Layer(l)) => {
                        layer_min = self.puzzle.n - 1 - 2 * l;
                        layer_max = self.puzzle.n - 1 - 2 * l;
                    }
                    Some(TurnLayer::WholePuzzle) => unreachable!(),
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
            AppMode::Turn => self.current_keys.clone(),
            AppMode::LiveFilter => format!("live filter: {}", self.live_filter_string),
        }
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
}

fn main_inner() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut state;
    if let Some(log_file) = args.log {
        let file = File::open(log_file)?;
        let reader = BufReader::new(file);
        let app_log = serde_json::from_reader(reader).map_err(std::io::Error::other)?;
        state = AppState::from_app_log(app_log);
    } else {
        let Some(n) = args.n else {
            return Err("n must be specified".into());
        };
        let Some(d) = args.d else {
            return Err("d must be specified".into());
        };

        if d > MAX_DIM {
            return Err(format!("dimension should be less than or equal to {MAX_DIM}").into());
        }
        if d < 1 {
            return Err("dimension should be greater than 0".into());
        }
        if n > MAX_LAYERS {
            return Err(format!("side should be less than or equal to {MAX_LAYERS}").into());
        }
        if d < 1 {
            return Err("side should be greater than 0".into());
        }

        state = AppState::new(n, d);
    }

    if let Some(path) = args.filters {
        let filters_str = std::fs::read_to_string(path).expect("Invalid filter file");
        state.filters = filters_str.lines().map(|l| l.parse().unwrap()).collect();
    }

    let layout = Layout::make_layout(state.puzzle.n, state.puzzle.d, args.compact, args.vertical)
        .move_right(1);
    //println!("{:?}", layout.keybind_hints);
    //return Ok(());

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;
    stdout.execute(cursor::Hide)?;

    loop {
        let frame_begin = Instant::now();

        let previous_message = state.get_message();
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                modifiers,
                ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Char(c) => {
                        state.process_key(c, modifiers);
                    }
                    KeyCode::Tab => {
                        state.process_key('\t', modifiers);
                    }
                    KeyCode::Esc => {
                        state.process_key(ESCAPE_CODE, modifiers);
                    }
                    KeyCode::Enter => {
                        state.process_key('\n', modifiers);
                    }
                    KeyCode::Backspace => {
                        state.process_key(BACKSPACE_CODE, modifiers);
                    }
                    _ => (),
                }
            }
        }

        let message = state.get_message();

        if previous_message != message {
            stdout
                .queue(cursor::MoveTo(0, layout.height))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .flush()?;

            stdout
                .queue(cursor::MoveTo(0, layout.height))?
                .queue(style::Print(message))?;
        }

        for ((x, y), pos) in &layout.points {
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
                    POS_NAMES[side as usize]
                } else {
                    NEG_NAMES[(!side) as usize]
                };
                color = if !in_filter {
                    FILTERED_COLOR
                } else if side >= 0 {
                    POS_COLORS[side as usize]
                } else {
                    NEG_COLORS[(!side) as usize]
                };
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            } else if !matches!(layout.keybind_hints.get(&(*x, *y)), Some(Some(_))) {
                if state.alert % (ALERT_FRAMES * 2) >= ALERT_FRAMES {
                    ch = '+';
                    color = ALERT_COLOR;
                } else {
                    ch = '·';
                    color = if in_filter {
                        PIECE_COLOR
                    } else {
                        FILTERED_COLOR
                    };
                }
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }
        }

        for ((x, y), side) in &layout.keybind_hints {
            // in this loop we are more efficient by not flushing the buffer.
            let ch;
            let color;
            if let Some(side) = side {
                ch = if state.current_turn.side.is_none()
                    || (state.keybind_set == KeybindSet::FixedKey && state.puzzle.d == 3)
                {
                    if *side >= 0 {
                        POS_KEYS[*side as usize]
                    } else {
                        NEG_KEYS[(!side) as usize]
                    }
                } else {
                    match state.keybind_axial {
                        KeybindAxial::Axial => {
                            if *side >= 0 {
                                AXIS_KEYS[*side as usize]
                            } else {
                                '·'
                            }
                        }
                        KeybindAxial::Side => {
                            if *side >= 0 {
                                POS_KEYS_RIGHT[*side as usize]
                            } else {
                                NEG_KEYS_RIGHT[(!side) as usize]
                            }
                        }
                    }
                };
                color = PIECE_COLOR;

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
            sleep(FRAME_LENGTH - frame);
        }
        //state.puzzle.turn(0, 2, 2, 1); // R
    }

    terminal::disable_raw_mode()?; // does this help?
    Ok(())
}

fn main() {
    let res = main_inner();
    if let Err(err) = res {
        println!("{}", err);
    }
}
