use crossterm::{
    cursor, queue,
    style::{self, Color, Stylize},
    terminal, ExecutableCommand,
};
use layout::Layout;
use puzzle::Puzzle;
use std::env;
use std::io::{self, Write};

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

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let n = args[1].parse().expect("must be integer");
    let d = args[2].parse().expect("must be integer");
    if n > 6 {
        panic!("dimension should be less than or equal to 6")
    }
    let puzzle = Puzzle::make_solved(n, d);
    let layout = Layout::make_layout(n, d);

    /*println!("{:?}", puzzle);
    println!("{:?}", layout);
    return Ok(());*/

    let mut stdout = io::stdout();
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    for ((x, y), pos) in layout.points {
        // in this loop we are more efficient by not flushing the buffer.
        let ch;
        let color;
        if pos.iter().any(|x| x.abs() == n) {
            let side = puzzle.stickers[&pos];
            if side >= 0 {
                ch = POS_NAMES[side as usize];
                color = POS_COLORS[side as usize];
            } else {
                ch = NEG_NAMES[(!side) as usize];
                color = NEG_COLORS[(!side) as usize];
            }
        } else {
            ch = "·";
            color = PIECE_COLOR;
        }
        queue!(
            stdout,
            cursor::MoveTo(x as u16, y as u16),
            style::PrintStyledContent(ch.with(color))
        )?;
    }

    stdout.flush()?;
    queue!(stdout, cursor::MoveTo(0, layout.height as u16),)?;

    Ok(())
}
