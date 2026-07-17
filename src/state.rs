use crate::filters::Filter;
use crate::layout::{Layout, ScreenLocation};
use crate::prefs::KeyCommandTurnRotate;
use crate::prefs::{
    self, KeyCommand, KeyCommandHandle, KeyCommandLayer, KeyCommandRotate, KeyCommandSection,
    KeyCommandSide, KeyCommandSideMode, KeyCommandTurn, keycode_name_char,
};
use crate::prefs::{Prefs, keycode_name};
use crate::puzzle::{Axis, Side};
use crate::puzzle::{Position, TurnBlock};
use crate::puzzle::{Puzzle, Turn};
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
pub struct TurnLayer {
    min: i16,
    max: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TurnBuild {
    pub layer: Option<TurnLayer>,
    pub sides: Option<TurnBuildSides>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TurnBuildSides {
    pub side: Option<Side>, // if none it is a whole puzzle rotation
    pub handles: TurnBuildHandles,
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TurnBuildHandles {
    Simple(TurnBuildHandlesSimple),
    Fixed(TurnBuildHandlesFixed),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TurnBuildHandlesSimple {
    pub from: Option<Side>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TurnBuildHandlesFixed {
    pub fixed: Vec<Side>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeybindAxial {
    Axial, // select axes, fewer keys
    Side,  // select sides, more keys
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AppMode {
    #[default]
    Turn,
    LiveFilter,
    KeybindMenu,
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
    pub rng: ThreadRng,
    pub keybind_layer: usize,
    pub message: Option<String>,
    pub undo_history: Vec<Turn>,
    pub redo_history: Vec<Turn>,
    pub filters: Vec<Filter>,
    pub filter_ind: usize,
    pub use_live_filter: bool,
    pub live_filter_string: String,
    pub live_filter_pending: Filter,
    pub live_filter: Filter,
    pub hovered: Option<ScreenLocation>,
    pub clicked: Vec<Position>,
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
    fn new(n: Option<i16>, d: Option<i16>, prefs: Prefs) -> Result<Self, String> {
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
            ));
        }
        if d < 1 {
            return Err("dimension should be greater than 0".into());
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
            rng: rand::thread_rng(),
            keybind_layer: 0,
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

    fn flush_turn(&mut self) {
        self.current_keys = Vec::new();
        self.current_turn = Default::default();
        self.live_filter_string = Default::default();
    }

    fn reset_mode(&mut self) {
        self.flush_turn();
        self.mode = Default::default();
        self.message = None;
    }

    pub fn make_layout(&self, compact: bool, vertical: bool) -> Layout {
        Layout::make_layout(
            self.puzzle.n,
            self.puzzle.d - self.section.len() as i16,
            compact,
            vertical,
        )
        .move_right(1)
    }

    pub fn allowed_live_filter_characters(&self) -> Vec<char> {
        let mut out = vec!['+', '!'];
        out.extend(crate::filters::DIGITS.chars());
        out.extend(
            self.prefs
                .axes
                .iter()
                .take(self.puzzle.d as usize)
                .map(|axis| axis.pos.name),
        );
        out.extend(
            self.prefs
                .axes
                .iter()
                .take(self.puzzle.d as usize)
                .map(|axis| axis.neg.name),
        );
        out
    }

    pub fn current_keybinds(&self) -> HashMap<KeyCode, KeyCommand> {
        let mut out = self.prefs.keys.global.clone();
        out.extend(self.prefs.keys.layers[self.keybind_layer].keys.clone());
        out
    }

    pub fn process_command(&mut self, command: &KeyCommand, code: KeyCode) {
        self.message = None;

        match self.mode {
            AppMode::LiveFilter => {
                if let KeyCode::Char(ch) = code
                    && self.allowed_live_filter_characters().contains(&ch)
                {
                    self.live_filter_string.push(ch);
                } else if code == KeyCode::Backspace {
                    self.live_filter_string.pop();
                } else {
                    match command {
                        KeyCommand::Side(subcommand) => {
                            let side = subcommand.side;
                            if self.has_side(side) {
                                self.live_filter_string
                                    .push(self.prefs.side_prefs(side).name);
                            }
                        }
                        KeyCommand::ResetMode => self.reset_mode(),
                        _ => {}
                    }
                }

                let filter_result: Result<Filter, _> =
                    Filter::parse(&self.live_filter_string, &self.prefs);
                if let Ok(filter) = &filter_result {
                    self.live_filter_pending = filter.clone();
                }

                if code == KeyCode::Enter {
                    if let Err(err) = filter_result {
                        self.message = Some(err);
                    } else {
                        self.flush_turn();
                        self.mode = Default::default();
                        self.use_live_filter = true;
                        self.live_filter = self.live_filter_pending.clone();
                    }
                }
            }
            AppMode::KeybindMenu => {
                if let Some((i, layer)) = self
                    .prefs
                    .keys
                    .layers
                    .iter()
                    .enumerate()
                    .find(|(_i, layer)| layer.menu == code)
                {
                    self.keybind_layer = i;
                    self.set_message(&format!("set keybinds to {}", layer.name));
                    self.mode = Default::default();
                }
            }
            AppMode::Turn => match command {
                KeyCommand::Null => {}
                KeyCommand::KeybindCycle => {
                    self.flush_turn();
                    self.keybind_layer += 1;
                    self.keybind_layer =
                        self.keybind_layer.rem_euclid(self.prefs.keys.layers.len());
                    self.message = Some(format!(
                        "set keybinds to {}",
                        self.prefs.keys.layers[self.keybind_layer].name
                    ))
                }
                KeyCommand::KeybindMenu => {
                    self.mode = AppMode::KeybindMenu;
                }
                KeyCommand::Undo => {
                    self.flush_turn();
                    let undid = self.undo_history.pop();
                    match undid {
                        None => {
                            self.set_message("nothing to undo");
                        }
                        Some(undid) => {
                            self.puzzle.turn(undid.inverse());
                            self.redo_history.push(undid)
                        }
                    }
                }
                KeyCommand::Redo => {
                    self.flush_turn();
                    let redid = self.redo_history.pop();
                    match redid {
                        None => {
                            self.set_message("nothing to redo");
                        }
                        Some(redid) => {
                            self.puzzle.turn(redid);
                            self.undo_history.push(redid)
                        }
                    }
                }
                KeyCommand::NextFilter => {
                    if self.filters.is_empty() {
                        self.set_message("no filters loaded");
                    } else {
                        self.flush_turn();
                        self.filter_ind += 1;
                        self.filter_ind = self.filter_ind.rem_euclid(self.filters.len());
                        self.use_live_filter = false;
                        self.set_message("next filter");
                    }
                }
                KeyCommand::PrevFilter => {
                    if self.filters.is_empty() {
                        self.set_message("no filters loaded");
                    } else {
                        self.flush_turn();
                        self.filter_ind -= 1;
                        self.filter_ind = self.filter_ind.rem_euclid(self.filters.len());
                        self.use_live_filter = false;
                        self.set_message("previous filter");
                    }
                }
                KeyCommand::LiveFilterMode => {
                    self.mode = AppMode::LiveFilter;
                }
                KeyCommand::ResetMode => self.reset_mode(),
                KeyCommand::Save => match self.save() {
                    Ok(()) => self.set_message(&format!("saved to {}", self.filename.display())),
                    Err(_err) => self.set_message("failed to save"),
                },
                KeyCommand::Layer(KeyCommandLayer { layer }) => {
                    if *layer > self.puzzle.n {
                        return;
                    }

                    if self.current_turn.sides.is_some() {
                        self.current_keys.push(code);
                        self.current_turn.layer = Some(TurnLayer {
                            min: self.puzzle.n - 1 - 2 * (layer - 1),
                            max: self.puzzle.n - 1 - 2 * (layer - 1),
                        });
                    }
                }
                KeyCommand::Section(KeyCommandSection { axis, direction }) => {
                    let sign = direction.n();
                    if let Some(sec) = self.section.get_mut(axis.0 as usize) {
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
                }
                KeyCommand::Side(KeyCommandSide { mode, strict, side }) => {
                    if let Some(sides) = &self.current_turn.sides
                        && sides.strict
                    {
                        self.add_handle(*side, code);
                    } else {
                        if self.current_turn.sides.is_some() {
                            self.flush_turn();
                        }

                        match mode {
                            KeyCommandSideMode::Simple => {
                                self.current_turn.sides = Some(TurnBuildSides {
                                    side: Some(*side),
                                    handles: TurnBuildHandles::Simple(Default::default()),
                                    strict: *strict,
                                })
                            }
                            KeyCommandSideMode::Fixed => {
                                self.current_turn.sides = Some(TurnBuildSides {
                                    side: Some(*side),
                                    handles: TurnBuildHandles::Fixed(Default::default()),
                                    strict: *strict,
                                })
                            }
                        };
                        self.current_keys.push(code);
                    }
                }
                KeyCommand::Rotate(KeyCommandRotate { mode, strict }) => match mode {
                    KeyCommandSideMode::Simple => {
                        self.current_turn.sides = Some(TurnBuildSides {
                            side: None,
                            handles: TurnBuildHandles::Simple(Default::default()),
                            strict: *strict,
                        })
                    }
                    KeyCommandSideMode::Fixed => {
                        self.current_turn.sides = Some(TurnBuildSides {
                            side: None,
                            handles: TurnBuildHandles::Fixed(Default::default()),
                            strict: *strict,
                        })
                    }
                },
                KeyCommand::Handle(KeyCommandHandle { side }) => self.add_handle(*side, code),
                KeyCommand::Turn(KeyCommandTurn {
                    side,
                    layer_min,
                    layer_max,
                    from,
                    to,
                }) => {
                    self.flush_turn();
                    if let Some(min) = layer_min
                        && let Some(max) = layer_max
                    {
                        self.current_turn.layer = Some(TurnLayer {
                            min: *min,
                            max: *max,
                        })
                    };
                    self.current_turn.sides = Some(TurnBuildSides {
                        side: Some(*side),
                        handles: TurnBuildHandles::Simple(TurnBuildHandlesSimple {
                            from: Some(from.pos_side()),
                        }),
                        strict: true,
                    });
                    self.add_handle(to.pos_side(), code);
                }
                KeyCommand::TurnRotate(KeyCommandTurnRotate { from, to }) => {
                    self.flush_turn();
                    self.current_keys.push(code);
                    self.perform_turn(Turn {
                        block: None,
                        from: from.pos_side(),
                        to: to.pos_side(),
                    });
                }
            },
        }
    }

    fn add_handle(&mut self, handle: Side, code: KeyCode) {
        let Some(sides) = self.current_turn.sides.as_mut() else {
            return;
        };

        let layer = self.current_turn.layer.unwrap_or(TurnLayer {
            min: self.puzzle.n - 1,
            max: self.puzzle.n - 1,
        });
        let block = sides.side.map(|side| TurnBlock {
            side,
            layer_min: layer.min,
            layer_max: layer.max,
        });

        let turn_performed;
        match &mut sides.handles {
            TurnBuildHandles::Simple(TurnBuildHandlesSimple { from }) => match from {
                None => {
                    if let Some(side) = sides.side
                        && side.axis() == handle.axis()
                    {
                        self.start_alert();
                        return;
                    }
                    *from = Some(handle);
                    turn_performed = false;
                }
                Some(from) => {
                    if from.axis() == handle.axis() {
                        self.start_alert();
                        return;
                    }
                    if let Some(side) = sides.side
                        && (side.axis() == from.axis() || side.axis() == handle.axis())
                    {
                        self.start_alert();
                        return;
                    }

                    let from_val = *from;
                    self.perform_turn(Turn {
                        block,
                        from: from_val,
                        to: handle,
                    });
                    turn_performed = true;
                }
            },
            TurnBuildHandles::Fixed(TurnBuildHandlesFixed { fixed }) => {
                if self.puzzle.d <= 3 {
                    return;
                }

                let mut sign = 1;
                let mut axes_perm = Vec::new();
                for h in sides.side.iter().chain(fixed.iter()) {
                    axes_perm.push(h.axis());
                    if !h.is_pos() {
                        sign *= -1;
                    }
                }

                if (axes_perm.len() as i16) < self.puzzle.d - 2 {
                    if fixed.iter().any(|h| h.axis() == handle.axis()) {
                        self.start_alert();
                        return;
                    }
                    if let Some(side) = sides.side
                        && side.axis() == handle.axis()
                    {
                        self.start_alert();
                        return;
                    }

                    fixed.push(handle);
                    turn_performed = false;
                } else {
                    let mut remaining = self.puzzle.axes().filter(|a| !axes_perm.contains(a));
                    let from = remaining.next().unwrap();
                    let to = remaining.next().unwrap();
                    for (i, ax1) in axes_perm.iter().enumerate() {
                        for ax2 in &axes_perm[..i] {
                            if ax1.0 > ax2.0 {
                                sign *= -1;
                            }
                        }
                    }
                    if sign == 1 {
                        self.perform_turn(Turn {
                            block,
                            from: from.pos_side(),
                            to: to.pos_side(),
                        });
                    } else {
                        self.perform_turn(Turn {
                            block,
                            from: to.pos_side(),
                            to: from.pos_side(),
                        });
                    }

                    turn_performed = true;
                }
            }
        }

        self.current_keys.push(code);

        let Some(sides) = self.current_turn.sides.as_mut() else {
            return;
        }; // borrow checker makes me do it again

        if turn_performed {
            if sides.strict {
                self.current_turn = Default::default()
            } else {
                match &mut sides.handles {
                    TurnBuildHandles::Simple(handles) => {
                        handles.from = None;
                    }
                    TurnBuildHandles::Fixed(handles) => {
                        handles.fixed = Vec::new();
                    }
                }
            }
        }
    }

    fn perform_turn(&mut self, turn: Turn) -> Option<()> {
        turn.validate()?;

        // turn clicked stickers
        for clicked in &mut self.clicked {
            clicked.apply_turn(turn);
        }

        self.undo_history.push(turn);
        self.puzzle.turn(turn);

        if self.puzzle.is_solved() {
            self.set_message("solved!");
        }

        Some(())
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
            AppMode::KeybindMenu => "select keybind set".to_string(),
        }
    }

    fn set_message(&mut self, message: &str) {
        self.message = Some(message.to_string());
    }

    fn clicked_stickers(&self) -> HashMap<Position, ClickedStyle> {
        let mut out = HashMap::new();
        for clicked in &self.clicked {
            let body = self.puzzle.piece_body(clicked);
            out.insert(body.clone(), ClickedStyle::OnPiece);
            for sticker in self.puzzle.piece_body_stickers(&body) {
                out.insert(sticker, ClickedStyle::OnPiece);
            }
            out.insert(clicked.clone(), ClickedStyle::Clicked);
        }
        out
    }

    pub fn has_axis(&self, axis: Axis) -> bool {
        axis.in_dimension(self.puzzle.d)
    }

    pub fn has_side(&self, side: Side) -> bool {
        side.in_dimension(self.puzzle.d)
    }

    fn start_alert(&mut self) {
        self.alert = self.prefs.alert_frames * 4 - 1;
    }

    fn which_keybind_hints(&self) -> KeybindAxial {
        match &self.current_turn.sides {
            Some(sides) => {
                if sides.strict {
                    KeybindAxial::Side
                } else {
                    KeybindAxial::Axial
                }
            }
            None => KeybindAxial::Side,
        }
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
    d: Option<i16>,

    /// Start with a new scramble
    #[arg(short, long)]
    scrambled: bool,

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
        if args.scrambled {
            if state.puzzle.d <= 2 {
                state.set_message("could not scramble");
            } else {
                state.puzzle.scramble(&mut state.rng);
                state.set_message("scrambled with 5000 turns");
                state.scramble = state.puzzle.clone();
            }
        }
    }

    state.set_section(args.section);

    if let Some(path) = args.filters {
        let filters_str = std::fs::read_to_string(path).expect("Invalid filter file");
        state.filters = filters_str
            .lines()
            .map(|l| Filter::parse(l, &state.prefs).unwrap())
            .collect();
    }

    let layout = state.make_layout(args.compact, args.vertical);

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
                        let b = state.current_keybinds();
                        let command = b.get(&code).unwrap_or(&KeyCommand::Null);
                        state.process_command(command, code);
                    }
                },
                Event::Mouse(MouseEvent {
                    kind, column, row, ..
                }) => {
                    let key = ScreenLocation::new(column as i16, row as i16);
                    let sticker = layout.points.get(&key);
                    if let Some(sticker) = sticker {
                        let mut sticker = sticker.clone();
                        sticker.0.extend(state.section.iter());
                        let sticker = sticker;

                        match kind {
                            MouseEventKind::Down(_button) => {
                                let original_length = state.clicked.len();
                                state.clicked.retain(|st| {
                                    st.0.iter()
                                        .zip(sticker.0.iter())
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
                .queue(cursor::MoveTo(0, layout.dimensions.y as u16))?
                .queue(terminal::Clear(terminal::ClearType::All))?
                .flush()?;
        }
        if previous_message != message {
            stdout
                .queue(cursor::MoveTo(0, layout.dimensions.y as u16))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .queue(style::Print(message))?;
        }

        if let Some(ScreenLocation { x, y }) = previous_hovered {
            erase_brackets(&mut stdout, x, y)?;
        }

        if let Some(ScreenLocation { x, y }) = state.hovered {
            draw_brackets(&mut stdout, x, y, ClickedStyle::Hovered, &state.prefs)?;
        }

        let clicked_stickers = state.clicked_stickers();
        let mut clicked_locs: HashMap<_, _> = CLICKED_STYLES
            .iter()
            .map(|s| (*s, HashSet::new()))
            .collect();

        for (loc @ ScreenLocation { x, y }, pos) in &layout.points {
            // in this loop we are more efficient by not flushing the buffer.

            let mut pos = pos.clone();
            pos.0.extend(state.section.iter());
            if !state.puzzle.is_sticker_or_piece(&pos) {
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

            let in_filter = filter.matches_stickers(&state.puzzle.piece_sticker_colors(&pos));

            if let Some(side) = state.puzzle.stickers.get(&pos) {
                ch = if args.boxes {
                    '■'
                } else {
                    state.prefs.side_prefs(*side).name
                };
                color = if !in_filter {
                    state.prefs.global_colors.filtered
                } else {
                    state.prefs.side_prefs(*side).color
                };
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            } else if !matches!(layout.keybind_hints.get(loc), Some(Some(_))) {
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

        for (loc @ ScreenLocation { x, y }, side) in &layout.keybind_hints {
            let mut pos = layout.points[loc].clone();
            pos.0.extend(state.section.iter());
            if state.puzzle.is_sticker(&pos) {
                continue;
            }

            // in this loop we are more efficient by not flushing the buffer.
            let ch;
            let color;
            if let Some(side) = side {
                let code = match state.which_keybind_hints() {
                    KeybindAxial::Axial => {
                        state
                            .current_keybinds()
                            .into_iter()
                            .find(|(_code, command)| matches!(command, KeyCommand::Handle(handle) if handle.side==*side))
                    }
                    KeybindAxial::Side => {
                        state
                            .current_keybinds()
                            .into_iter()
                            .find(|(_code, command)| matches!(command, KeyCommand::Side(handle) if handle.side==*side))
                    }
                };

                ch = if let Some((code, _)) = code {
                    keycode_name_char(code)
                } else {
                    '·'
                };

                color = state.prefs.global_colors.piece;

                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }
        }

        stdout
            .queue(cursor::MoveTo(0, layout.dimensions.y as u16))?
            .flush()?;

        if state.alert > 0 {
            state.alert -= 1;
        }

        let frame_end = Instant::now();
        let frame = frame_end - frame_begin;
        if frame < FRAME_LENGTH {
            std::thread::sleep(FRAME_LENGTH - frame);
        }

        persistent_clicked_locs = clicked_locs;
    }

    drop(stdout_manager);
    Ok(())
}
