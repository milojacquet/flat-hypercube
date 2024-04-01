use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    queue,
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use layout::Layout;
use puzzle::Puzzle;
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
const POS_KEYS: &'static [char] = &['f', 'e', 's', 'v', 't', 'y'];
const NEG_KEYS: &'static [char] = &['w', 'c', 'r', 'd', 'g', 'h'];
const AXIS_KEYS: &'static [char] = &['k', 'j', 'l', 'i', 'u', 'o'];
const LAYER_KEYS: &'static [char] = &['1', '2', '3', '4', '5', '6', '7', '8'];
const ROT_KEY: char = 'x';
const POS_COLORS: &'static [Color] = &[
    hex(0xff0000),
    hex(0xffffff),
    hex(0x00ff00),
    hex(0xff00ff),
    hex(0x0aaa85),
    hex(0x774811),
];
const NEG_COLORS: &'static [Color] = &[
    hex(0xff8000),
    hex(0xffff00),
    hex(0x0080ff),
    hex(0x8f10ea),
    hex(0x7daa0a),
    hex(0x6d4564),
];
const PIECE_COLOR: Color = hex(0x808080);
const ALERT_COLOR: Color = hex(0xd86c6c);
const FRAME_LENGTH: Duration = Duration::from_millis(1000 / 30);
const ALERT_FRAMES: u8 = 4;

enum TurnLayer {
    Layer(i16),
    WholePuzzle,
}

#[derive(Default)]
struct TurnBuild {
    layer: Option<TurnLayer>,
    side: Option<i16>,
    from: Option<i16>, // no need to remember to because that immediately triggers the move
}

struct AppState {
    puzzle: Puzzle,
    current_keys: String,
    current_turn: TurnBuild,
    alert: u8,
}

impl AppState {
    fn flush_turn(&mut self) {
        self.current_keys = "".to_string();
        self.current_turn = Default::default();
    }

    fn process_key(&mut self, c: char) {
        if let Some(s) = LAYER_KEYS.iter().position(|ch| ch == &c) {
            if s as i16 >= self.puzzle.n {
                return;
            }
            self.current_keys.push(c);
            self.current_turn.layer = Some(TurnLayer::Layer(s as i16));
        } else if c == ROT_KEY {
            self.current_keys.push(c);
            self.current_turn.layer = Some(TurnLayer::WholePuzzle);
        } else if let Some(s) = POS_KEYS.iter().position(|ch| ch == &c) {
            if s as u16 >= self.puzzle.d {
                return;
            }
            self.flush_turn();
            self.current_keys.push(c);
            self.current_turn.side = Some(s as i16);
        } else if let Some(s) = NEG_KEYS.iter().position(|ch| ch == &c) {
            if s as u16 >= self.puzzle.d {
                return;
            }
            self.flush_turn();
            self.current_keys.push(c);
            self.current_turn.side = Some(!(s as i16));
        } else if let (Some(side), Some(s)) = (
            self.current_turn.side,
            AXIS_KEYS.iter().position(|ch| ch == &c),
        ) {
            if s as u16 >= self.puzzle.d {
                return;
            }
            self.current_keys.push(c);
            if let Some(from) = self.current_turn.from {
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
                    Some(TurnLayer::WholePuzzle) => {
                        layer_min = -self.puzzle.n + 1;
                        layer_max = self.puzzle.n - 1;
                    }
                }
                if side < 0 {
                    layer_min *= -1;
                    layer_max *= -1;
                    std::mem::swap(&mut layer_min, &mut layer_max)
                };
                match self.puzzle.turn(side, layer_min, layer_max, from, s as i16) {
                    None => {
                        self.alert = ALERT_FRAMES * 4 - 1;
                        self.current_keys =
                            self.current_keys[..self.current_keys.len() - 2].to_string();
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

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let n = args[1].parse().expect("must be integer");
    let d = args[2].parse().expect("must be integer");
    if n > 6 {
        panic!("dimension should be less than or equal to 6")
    }
    let mut state = AppState {
        puzzle: Puzzle::make_solved(n, d),
        current_keys: "".to_string(),
        current_turn: Default::default(),
        alert: 0,
    };
    let layout = Layout::make_layout(n, d);

    // puzzle.turn(0, 2, 2, 1); // R
    // puzzle.turn(1, 2, 0, 2); // U

    /*println!("{:?}", puzzle);
    println!("{:?}", layout);
    return Ok(());*/

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;
    stdout.execute(cursor::Hide)?;

    loop {
        let frame_begin = Instant::now();
        //stdout.execute(terminal::Clear(terminal::ClearType::Purge))?; // don't queue this
        /*for y in 0..layout.height {
            stdout
                .queue(cursor::MoveTo(0, y as u16))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?;
        }
        stdout.flush()?;*/

        let previous_keys = state.current_keys.clone();
        if event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                }) => match code {
                    KeyCode::Char(c) => {
                        state.process_key(c);
                    }
                    KeyCode::Esc => {
                        break ();
                    }
                    _ => (),
                },
                _ => (),
            }
        }

        if previous_keys != state.current_keys {
            stdout
                .queue(cursor::MoveTo(0, layout.height as u16))?
                .queue(terminal::Clear(terminal::ClearType::CurrentLine))?
                .flush()?;

            stdout
                .queue(cursor::MoveTo(0, layout.height as u16))?
                .queue(style::Print(state.current_keys.clone()))?;
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
            } else {
                if state.alert % (ALERT_FRAMES * 2) >= ALERT_FRAMES {
                    ch = "+";
                    color = ALERT_COLOR;
                } else {
                    ch = "·";
                    color = PIECE_COLOR;
                }
            }
            stdout
                .queue(cursor::MoveTo(*x as u16, *y as u16))?
                .queue(style::PrintStyledContent(ch.with(color)))?;
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

    Ok(())
}
