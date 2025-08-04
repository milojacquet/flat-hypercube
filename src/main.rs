use crate::prefs::BACKSPACE_CODE;
use crate::prefs::ESCAPE_CODE;
use clap::Parser;
use crossterm::{
    cursor,
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    },
    style::{self, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use filters::Filter;
use layout::Layout;
use prefs::Prefs;
use puzzle::{ax, Puzzle, PuzzleTurn, SideTurn, Turn};
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::{self, Write};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

mod filters;
mod layout;
mod prefs;
mod puzzle;

const FRAME_LENGTH: Duration = Duration::from_millis(1000 / 30);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickedStyle {
    Clicked,
    OnPiece,
    Hovered,
}

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
    hovered: Option<(i16, i16)>,
    clicked: Vec<Vec<i16>>,
    filename: PathBuf,
    prefs: Prefs,
}

#[derive(Serialize, Deserialize)]
struct AppLog {
    scramble: Puzzle,
    moves: Vec<Turn>,
}

impl AppState {
    fn new(n: i16, d: u16, prefs: Prefs) -> Self {
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
            hovered: None,
            clicked: Vec::new(),
            filename: Self::new_filename(),
            prefs,
        }
    }

    fn to_app_log(&self) -> AppLog {
        AppLog {
            scramble: self.scramble.clone(),
            moves: self.undo_history.clone(),
        }
    }

    fn from_app_log(app_log: AppLog, prefs: Prefs) -> Self {
        let mut state = AppState::new(app_log.scramble.n, app_log.scramble.d, prefs);
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
                    } else if let Some(s) = self
                        .prefs
                        .axes
                        .iter()
                        .position(|ax| ax.pos.keys.select == c)
                    {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                            self.flush_modes();
                        }
                        self.current_keys.push(c);
                        self.current_turn.side = Some(s as i16);
                        just_pressed_side = true;
                    } else if let Some(s) = self
                        .prefs
                        .axes
                        .iter()
                        .position(|ax| ax.neg.keys.select == c)
                    {
                        if s as u16 >= self.puzzle.d {
                            return;
                        }
                        if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                            self.flush_modes();
                        }
                        self.current_keys.push(c);
                        self.current_turn.side = Some(!(s as i16));
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
                                            self.alert = self.prefs.alert_frames * 4 - 1;
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

    fn get_axis_key(&self, c: char) -> Option<i16> {
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

    /// Preferences file
    #[arg(short, long)]
    prefs: Option<PathBuf>,
}

fn main_inner() -> Result<(), Box<dyn std::error::Error>> {
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
        let Some(n) = args.n else {
            return Err("n must be specified".into());
        };
        let Some(d) = args.d else {
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

        state = AppState::new(n, d, prefs);
    }

    if let Some(path) = args.filters {
        let filters_str = std::fs::read_to_string(path).expect("Invalid filter file");
        state.filters = filters_str
            .lines()
            .map(|l| Filter::parse(&l, &state.prefs).unwrap())
            .collect();
    }

    let layout = Layout::make_layout(state.puzzle.n, state.puzzle.d, args.compact, args.vertical)
        .move_right(1);
    //println!("{:?}", layout.keybind_hints);
    //return Ok(());

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(cursor::Hide)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;

    let stdout_manager = StdoutManager;

    'event: loop {
        let previous_message = state.get_message();
        let previous_hovered = state.hovered;
        let previous_clicked_stickers = state.clicked_stickers();
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
                },
                Event::Mouse(MouseEvent {
                    kind, column, row, ..
                }) => {
                    let key = (column as i16, row as i16);
                    let sticker = layout.points.get(&key);
                    if let Some(sticker) = sticker {
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
                .queue(style::Print(message))?;
        }

        if let Some((x, y)) = previous_hovered {
            erase_brackets(&mut stdout, x, y)?;
        }

        if let Some((x, y)) = state.hovered {
            draw_brackets(&mut stdout, x, y, ClickedStyle::Hovered, &state.prefs)?;
        }

        let clicked_stickers = state.clicked_stickers();

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

            if previous_clicked_stickers.get(pos) != clicked_stickers.get(pos) {
                erase_brackets(&mut stdout, *x, *y)?;
            }

            if let Some(style) = clicked_stickers.get(pos) {
                draw_brackets(&mut stdout, *x, *y, *style, &state.prefs)?;
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

    drop(stdout_manager);
    Ok(())
}

fn main() {
    let res = main_inner();
    if let Err(err) = res {
        println!("{}", err);
    }
}
