use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use layout::Layout;
use puzzle::{ax, Puzzle, PuzzleTurn, SideTurn, Turn};
use rand::prelude::*;
use std::env;
use std::io::{self, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

mod layout;
mod puzzle;

const fn hex(hex: u32) -> Color {
    Color::Rgb {
        r: ((hex >> 16) & 0xff) as u8,
        g: ((hex >> 8) & 0xff) as u8,
        b: ((hex >> 0) & 0xff) as u8,
    }
}

const POS_NAMES: &'static [&'static str] = &["R", "U", "F", "O", "A", "Γ", "Θ", "Ξ"];
const NEG_NAMES: &'static [&'static str] = &["L", "D", "B", "I", "P", "Δ", "Λ", "Π"];
const POS_KEYS: &'static [char] = &['f', 'e', 's', 'v', 't', 'y', 'n', 'q'];
const NEG_KEYS: &'static [char] = &['w', 'c', 'r', 'd', 'g', 'h', 'b', 'a'];
const POS_KEYS_RIGHT: &'static [char] = &['l', 'i', 'j', '.', 'p', '['];
const NEG_KEYS_RIGHT: &'static [char] = &['u', ',', 'o', 'k', 'l', ';'];
const AXIS_KEYS: &'static [char] = &['k', 'j', 'l', 'i', 'u', 'o', 'p', ';'];
const LAYER_KEYS: &'static [char] = &['1', '2', '3', '4', '5', '6', '7', '8', '9'];
const ROT_KEY: char = 'x';
const SCRAMBLE_KEY: char = '=';
const RESET_KEY: char = '-';
const DAMAGE_REPEAT: u8 = 5;
const KEYBIND_KEY: char = '\\';
const KEYBIND_AXIAL_KEY: char = '|';
const UNDO_KEY: char = 'z';
const REDO_KEY: char = 'Z';

const POS_COLORS: &'static [Color] = &[
    hex(0xff0000),
    hex(0xffffff),
    hex(0x00ff00),
    hex(0xff00ff),
    hex(0x0aaa85),
    hex(0x774811),
    hex(0xf49fef),
    hex(0xb29867),
];
const NEG_COLORS: &'static [Color] = &[
    hex(0xff8000),
    hex(0xffff00),
    hex(0x0080ff),
    hex(0x8f10ea),
    hex(0x7daa0a),
    hex(0x6d4564),
    hex(0xb29867),
    hex(0xb27967),
];
const PIECE_COLOR: Color = hex(0x808080);
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
            Self::FixedKey => Self::ThreeKey//Self::XyzKey,
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

struct AppState {
    puzzle: Puzzle,
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
}

impl AppState {
    fn flush_turn(&mut self) {
        self.current_keys = "".to_string();
        self.current_turn = Default::default();
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
            self.flush_turn();
            if ch == SCRAMBLE_KEY && self.puzzle.d >= 3 {
                self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                for _ in 0..5000 {
                    let mut axes: Vec<i16> = (0..self.puzzle.d as i16).collect();
                    axes.shuffle(&mut self.rng);
                    let layer = self.puzzle.n - 1 - 2 * self.rng.gen_range(0..self.puzzle.n);
                    self.puzzle.turn(Turn::Side(SideTurn {
                        side: axes[0],
                        layer_min: layer,
                        layer_max: layer,
                        from: axes[1],
                        to: axes[2],
                    }));
                    self.message = Some("scrambled with 5000 turns".to_string());
                }
                self.undo_history = vec![];
                self.redo_history = vec![];
            } else if ch == RESET_KEY {
                self.puzzle = Puzzle::make_solved(self.puzzle.n, self.puzzle.d);
                self.message = Some("puzzle reset".to_string());
                self.undo_history = vec![];
                self.redo_history = vec![];
            }
            self.damage_counter = None;
        }

        let mut just_pressed_side = false;

        if c == KEYBIND_KEY {
            self.flush_turn();
            self.keybind_set = self.keybind_set.next(self.puzzle.n);
            self.message = Some(format!("set keybinds to {}", self.keybind_set.name()))
        } else if c == KEYBIND_AXIAL_KEY {
            if self.puzzle.d > 6 {
                self.message = Some("not enough room for side keybinds".to_string());
            } else {
                self.flush_turn();
                self.keybind_axial = self.keybind_axial.next();
                self.message = Some(format!("set axis mode to {}", self.keybind_axial.name()))
            }
        } else if c == UNDO_KEY {
            self.flush_turn();
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
            self.flush_turn();
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
        } else if let Some(s) = LAYER_KEYS.iter().position(|ch| ch == &c) {
            if s as i16 >= self.puzzle.n {
                return;
            }
            self.flush_turn();
            self.current_keys.push(c);
            self.current_turn.layer = Some(TurnLayer::Layer(s as i16));
        } else if let Some(s) = POS_KEYS.iter().position(|ch| ch == &c) {
            if s as u16 >= self.puzzle.d {
                return;
            }
            if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                self.flush_turn();
            }
            self.current_keys.push(c);
            self.current_turn.side = Some(s as i16);
            just_pressed_side = true;
        } else if let Some(s) = NEG_KEYS.iter().position(|ch| ch == &c) {
            if s as u16 >= self.puzzle.d {
                return;
            }
            if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                self.flush_turn();
            }
            self.current_keys.push(c);
            self.current_turn.side = Some(!(s as i16));
            just_pressed_side = true;
        } else if c == ROT_KEY {
            if self.keybind_set == KeybindSet::ThreeKey {
                self.flush_turn();
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
                    if ax(s as i16) as u16 >= self.puzzle.d {
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
                            let turn_out = self.perform_turn(side, from, s as i16);

                            match turn_out {
                                None => {
                                    self.alert = ALERT_FRAMES * 4 - 1;
                                    self.current_keys = self.current_keys
                                        [..self.current_keys.len() - 2]
                                        .to_string();
                                }
                                _ => (),
                            }
                            self.current_turn.from = None;
                        } else {
                            self.current_turn.from = Some(s as i16);
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
                    if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                        self.flush_turn();
                    }
                    self.current_keys.push(c);
                    self.current_turn.side = Some(s as i16);
                    flip = true;
                    just_pressed_side = true;
                } else if let Some(s) = NEG_KEYS_RIGHT.iter().position(|ch| ch == &c) {
                    if ax(s as i16) as u16 >= self.puzzle.d {
                        return;
                    }
                    if self.current_turn.layer.is_none() || self.current_turn.side.is_some() {
                        self.flush_turn();
                    }
                    self.current_keys.push(c);
                    self.current_turn.side = Some(!(s as i16));
                    flip = true;
                    just_pressed_side = true;
                } else {
                    flip = false;
                }

                if let (Some(side), true) = (self.current_turn.side, just_pressed_side) {
                    if flip {
                        if side < 0 {
                            self.perform_turn(side, (!side + 1) % 3, (!side + 2) % 3);
                        } else {
                            self.perform_turn(side, (side + 2) % 3, (side + 1) % 3);
                        }
                    } else {
                        if side < 0 {
                            self.perform_turn(side, (!side + 2) % 3, (!side + 1) % 3);
                        } else {
                            self.perform_turn(side, (side + 1) % 3, (side + 2) % 3);
                        }
                    }
                }
            }
            KeybindSet::FixedKey => {
                let axis = self.get_axis_key(c);

                if let Some(s) = axis {
                    let s = s as i16;
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

                            match turn_out {
                                None => {
                                    self.alert = ALERT_FRAMES * 4 - 1;
                                    self.current_keys = self.current_keys
                                        [..self.current_keys.len() - self.current_turn.fixed.len()]
                                        .to_string();
                                }
                                _ => (),
                            }
                            self.current_turn.fixed = vec![];
                        }
                    }
                }
            } //_ => todo!(),
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
        self.message.clone().unwrap_or(self.current_keys.clone())
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let n = args[1].parse().expect("must be integer");
    let d = args[2].parse().expect("must be integer");
    let compact = args[3..].contains(&"--compact".to_string());
    if d > 8 {
        println!("dimension should be less than or equal to 8");
    }
    if d < 1 {
        panic!("dimension should be greater than 0");
    }
    if n > 19 {
        panic!("side should be less than or equal to 19");
    }
    if d < 1 {
        panic!("side should be greater than 0");
    }
    let mut state = AppState {
        puzzle: Puzzle::make_solved(n, d),
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
    };
    let layout = Layout::make_layout(n, d, compact).move_right(1);
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
            match event::read()? {
                Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    modifiers,
                    ..
                }) => match code {
                    KeyCode::Char(c) => {
                        state.process_key(c, modifiers);
                    }
                    KeyCode::Esc => {
                        break ();
                    }
                    _ => (),
                },
                _ => (),
            }
        }

        let message = state.get_message();

        if previous_message != message {
            stdout
                .queue(cursor::MoveTo(0, layout.height as u16))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .flush()?;

            stdout
                .queue(cursor::MoveTo(0, layout.height as u16))?
                .queue(style::Print(message))?;
        }

        for ((x, y), pos) in &layout.points {
            // in this loop we are more efficient by not flushing the buffer.
            let ch;
            let color;
            if pos.iter().any(|x| x.abs() == n) {
                let side = state.puzzle.stickers[pos];
                if side >= 0 {
                    ch = POS_NAMES[side as usize];
                    color = POS_COLORS[side as usize];
                } else {
                    ch = NEG_NAMES[(!side) as usize];
                    color = NEG_COLORS[(!side) as usize];
                }
                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            } else if !matches!(layout.keybind_hints.get(&(*x, *y)), Some(Some(_))) {
                if state.alert % (ALERT_FRAMES * 2) >= ALERT_FRAMES {
                    ch = "+";
                    color = ALERT_COLOR;
                } else {
                    ch = "·";
                    color = PIECE_COLOR;
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
                ch = match state.current_turn.side {
                    None => {
                        if *side >= 0 {
                            POS_KEYS[*side as usize]
                        } else {
                            NEG_KEYS[(!side) as usize]
                        }
                    }
                    Some(_) => match state.keybind_axial {
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
                    },
                };
                color = PIECE_COLOR;

                stdout
                    .queue(cursor::MoveTo(*x as u16, *y as u16))?
                    .queue(style::PrintStyledContent(ch.with(color)))?;
            }
            //state.message = format!("{:?}", (x, y, side)).into();
        }

        stdout
            .queue(cursor::MoveTo(0, layout.height as u16))?
            .flush()?;

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
