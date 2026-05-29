## Flat hypercube simulator

This is a program that allows you to solve hypercubes in any number of dimensions using a flat projection, by providing a custom prefs file that defines additional axes. It supports keybinds.

The projection is recursive in the number of dimensions. When a dimension is added, multiple copies of the previous dimension's puzzle are placed next to each other, along with a cap on either end. In each cap, the stickers from the lower-dimensional puzzle have been removed and the stickers have been replaced with pieces. The middle puzzles represent the layers of the puzzle along the new dimension, and the caps represent the two new facets added along this direction. The layout was inspired by Don Hatch's layout in [MagicCubeNdSolve](http://www.plunk.org/~hatch/MagicCubeNdSolve/).

### Use

To start the program, run it with the arguments `[n] [d]` to produce an `n^d` puzzle. Use `--compact` or `-c` to move the stickers closer to each other, which can help on smaller screens. You can pan the viewport by dragging with the mouse, using arrow keys, or scrolling with the touchpad.

This program supports multiple methods of interaction. In all modes, pressing <kbd>=</kbd> 5 times scrambles the puzzle, and pressing <kbd>-</kbd> 5 times resets the puzzle. <kbd>Ctrl</kbd>+<kbd>C</kbd> quits the program (press twice for forced quit). <kbd>Z</kbd> undoes the most recent move, and <kbd>Shift</kbd>+<kbd>Z</kbd> redoes it.

There are multiple systems to turn the puzzle. <kbd>\\</kbd> cycles between them. In all of them, using <kbd>1</kbd> through <kbd>9</kbd> before a turn sequence selects the layer of the puzzle starting from the outermost.

Each side has several keys that can be used to access it in different contexts. The selector is usually used at the beginning of the key combination to select which side to turn. The other set of keys is used to determine which direction the side should turn. When in axis mode, these keys only refer to the positive direction on each axis, and when in side mode, there are keys for both sides. Axis mode and side mode can be toggled with <kbd>Shift</kbd>+<kbd>\\</kbd>. The table below shows the default keybindings provided by `default_prefs.json`; custom prefs files can define different keys and additional axes.

| Side (+/-) | Selector | Axis mode | Side mode |
| -------- | ------- | ------- | ------- |
| R, L | <kbd>F</kbd>, <kbd>S</kbd> | <kbd>K</kbd> | <kbd>L</kbd>, <kbd>J</kbd> |
| U, D | <kbd>E</kbd>, <kbd>D</kbd> | <kbd>J</kbd> | <kbd>I</kbd>, <kbd>K</kbd> |
| F, B | <kbd>R</kbd>, <kbd>W</kbd> | <kbd>L</kbd> | <kbd>O</kbd>, <kbd>U</kbd> |
| O, I | <kbd>T</kbd>, <kbd>G</kbd> | <kbd>I</kbd> | <kbd>P</kbd>, <kbd>;</kbd> |
| A, P | <kbd>V</kbd>, <kbd>C</kbd> | <kbd>U</kbd> | <kbd>.</kbd>, <kbd>,</kbd> |
| Γ, Δ | <kbd>Y</kbd>, <kbd>H</kbd> | <kbd>O</kbd> | <kbd>[</kbd>, <kbd>'</kbd> |
| Θ, Λ | <kbd>N</kbd>, <kbd>B</kbd> | <kbd>P</kbd> | <kbd>N</kbd>, <kbd>M</kbd> |
| Ξ, Π | <kbd>Q</kbd>, <kbd>A</kbd> | <kbd>;</kbd> | <kbd>Y</kbd>, <kbd>H</kbd> |
| Σ, Φ | <kbd>,</kbd>, <kbd>M</kbd> | <kbd>[</kbd> | <kbd>T</kbd>, <kbd>G</kbd> |
| Ψ, Ω | <kbd>/</kbd>, <kbd>.</kbd> | <kbd>'</kbd> | <kbd>V</kbd>, <kbd>B</kbd> |

Keys within the same column must be unique (e.g. all selector keys must differ from each other), but keys *between* columns may overlap — a key can be a selector on one axis and an axis or side key on another.

#### Three-key mode

This mode is most similar to Magic Cube 7D. To make a turn, first use the side selector, then two axis keys to perform the turn that takes the first axis to the second axis. If you use <kbd>X</kbd> instead of the side selector, you can do a whole-puzzle rotation. Once you complete a move, you can continue to use axis keys to do additional moves on the same side.

**Three-key strict mode** requires every key in the sequence to be a selector key — no axis keys or side keys are used. After each turn the state is fully reset; layer selections and `x` do not persist between turns.

In **side mode**, whole-puzzle rotations respect the sign of each axis key: pressing a positive side key for one axis and a negative side key for the other produces the inverse rotation of pressing both positive.

#### Fixed-key mode

In this mode, first use a selector key to choose which side to turn, then press enough axis keys (or side keys in side mode) to fix the rotation to a plane. Once a move completes, you can continue to press axis keys for additional moves on the same side. For a whole-puzzle rotation, press <kbd>X</kbd> first — it replaces the selector and requires one extra axis key (d−2 total instead of d−3).

In three dimensions, just pressing a **selector** key immediately rotates that face counterclockwise; the negative selector key rotates it clockwise. Press <kbd>X</kbd> before the selector key to rotate the whole puzzle instead. Layer selections and <kbd>X</kbd> persist across turns until explicitly cleared by <kbd>Esc</kbd> or another layer key.

### Commutators and conjugators

<kbd>F1</kbd> starts a reversion block and <kbd>F2</kbd> ends it. The block is displayed on the second-to-last row as `RevStack: [n]` where `n` is the number of moves in the block.

- <kbd>F3</kbd> performs an **undo** of the block: applies the inverse of each move in reverse order.
- <kbd>F4</kbd> performs a **commutator**: applies the inverse of the block, then the inverse of all moves after the block. This is equivalent to undoing the moves before the block, performing them, then undoing everything — useful for checking commutativity.

The RevStack display adapts automatically when you undo or redo moves that cross block boundaries.

### Marking stickers

Click a sticker to mark it and its neighboring stickers with brackets. The marking happens on mouse **release**, so you can drag to pan without accidentally marking. Double-click on empty space to clear all marks.

### Saving and loading

Save the current session by using <kbd>Shift</kbd>+<kbd>S</kbd>. A session can be loaded by passing it in with `--log`.

### Piece filters

Flat hypercube supports passing piece filters from a file via the `--filters` option. Each line of the file should contain one filter. A filter consists of a sequence of terms separated by `+`, where each term consists of one or more selector characters, optionally followed by `!` and more selector characters. Each term shows pieces that match the selectors before the `!` and do not match the selectors after the `!`. A selector character can either be the name of a facet, which selects pieces with that facet's color, or a number from `0` to `9` or `&`, which represents 10, which selects pieces with that many colors. The filter shows all pieces that are shown in at least one term. To use the next filter, use <kbd>Shift</kbd>+<kbd>K</kbd>, and to use the previous filter, use <kbd>Shift</kbd>+<kbd>J</kbd>.

Live filter creation is also supported. To do this, use use <kbd>Shift</kbd>+<kbd>F</kbd> to enter live filter mode. Facet names are entered via their selector keybind in lowercase, or by typing their name in capital (Greek letters are supported), and `+`, `!`, and digits are entered normally. To confirm, use <kbd>Enter</kbd>, and to cancel, use <kbd>Esc</kbd>.

### Miscellaneous

The status message can be cleared and mode returned to default by pressing <kbd>Esc</kbd>.

## Use as a crate

Flat hypercube can be used as a crate. This funcationality is very rough. Use `AppState::process_key` to process input, `State::make_layout` to make the layout, and the `puzzle` field of `State` to read the current puzzle state.
