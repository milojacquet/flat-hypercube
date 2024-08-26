## Flat hypercube simulator

This is a program that allows you to solve hypercubes in up to 10 dimensions using a flat projection. It supports keybinds.

The projection is recursive in the number of dimensions. A puzzle of dimension 0 just has a single piece with no stickers: the core. When a dimension is added, multiple copies of the previous dimension's puzzle are placed next to each other, along with a cap on either end. In each cap, the stickers from the lower-dimensional puzzle have been removed and the stickers have been replaced with pieces. The middle puzzles represent the layers of the puzzle along the new dimension, and the caps represent the two new facets added along this direction. The layout was inspired by Don Hatch's layout in [MagicCubeNdSolve](http://www.plunk.org/~hatch/MagicCubeNdSolve/).

### Use

To start the program, download it from [releases](https://github.com/milojacquet/flat-hypercube/releases/latest) and run it with the arguments `[n] [d]` to produce an `n^d` puzzle. Use `--compact` to move the stickers closer to each other, which can help on smaller screens.

This program supports multiple methods of interaction. In all modes, pressing <kbd>=</kbd> 5 times scrambles the puzzle, and pressing <kbd>-</kbd> 5 times resets the puzzle. <kbd>Ctrl</kbd>+<kbd>C</kbd> quits the program. <kbd>Z</kbd> undoes the most recent move, and <kbd>Shift</kbd>+<kbd>Z</kbd> redoes it. 

There are multiple systems to turn the puzzle. <kbd>\\</kbd> cycles between them. In all of them, using <kbd>1</kbd> through <kbd>9</kbd> before a turn sequence selects the layer of the puzzle starting from the outermost.

Each side has several keys that can be used to access it in different contexts. The selector is usually used at the beginning of the key combination to select which side to turn. The other set of keys is used to determine which direction the side should turn. When in axis mode, these keys only refer to the positive direction on each axis, and when in side mode, there are keys for both sides. Axis mode and side mode can be toggled with <kbd>Shift</kbd>+<kbd>\\</kbd>. 
| Side (+/-) | Selector | Axis mode | Side mode |
| -------- | ------- | ------- | ------- |
| R, L | <kbd>F</kbd>, <kbd>S</kbd> | <kbd>K</kbd> | <kbd>L</kbd>, <kbd>U</kbd> |
| U, D | <kbd>E</kbd>, <kbd>D</kbd> | <kbd>J</kbd> | <kbd>I</kbd>, <kbd>,</kbd> |
| F, B | <kbd>R</kbd>, <kbd>W</kbd> | <kbd>L</kbd> | <kbd>J</kbd>, <kbd>O</kbd> |
| O, I | <kbd>T</kbd>, <kbd>G</kbd> | <kbd>I</kbd> | <kbd>.</kbd>, <kbd>K</kbd> |
| A, P | <kbd>V</kbd>, <kbd>C</kbd> | <kbd>U</kbd> | <kbd>P</kbd>, <kbd>L</kbd> |
| Γ, Δ | <kbd>Y</kbd>, <kbd>H</kbd> | <kbd>O</kbd> | <kbd>[</kbd>, <kbd>;</kbd> |
| Θ, Λ | <kbd>N</kbd>, <kbd>B</kbd> | <kbd>P</kbd> | N/A |
| Ξ, Π | <kbd>Q</kbd>, <kbd>A</kbd> | <kbd>;</kbd> | N/A |
| Σ, Φ | <kbd>,</kbd>, <kbd>M</kbd> | <kbd>[</kbd> | N/A |
| Ψ, Ω | <kbd>/</kbd>, <kbd>.</kbd> | <kbd>'</kbd> | N/A |

#### Three-key mode

This mode is most similar to Magic Cube 7D. To make a turn, first use the side selector, then two axis keys to perform the turn that takes the first axis to the second axis. If you use <kbd>X</kbd> instead of the side selector, you can do a whole-puzzle rotation. Once you complete a move, you can continue to use axis keys to do additional moves on the same side.

#### Fixed-key mode

In this mode, first use the side selector, then use enough axis keys to fix the rotation to occur in a plane. Once you complete a move, you can continue to use axis keys to do additional moves on the same side. To do a whole-puzzle rotation, include <kbd>X</kbd> somewhere in the sequence before the end. Once you complete a move, you can continue to use axis keys to do additional moves on the same side.

In three dimensions, just pressing a side selector key rotates that side counterclockwise. To rotate it clockwise, use the corresponding face selector key from side mode.

## Saving and loading

Not yet

## Piece filters

Flat hypercube supports passing piece filters from a file via the `--filters` flag. Each line of the file should contain one filter. A filter consisters of a sequence of terms separated by `+`, where each term consists of the names of some facets, optionally followed by `!` and more facets. Each term corresponds to a cuboidal filter that shows pieces that have colors named before the `!` and do not have colors named after the `!`, where the colors are named after their solved side. The filter shows all pieces that are shown in at least one term. To use the next filter, use <kbd>Shift</kbd>+<kbd>K</kbd>, and to use the previous filter, use <kbd>Shift</kbd>+<kbd>J</kbd>.

Live filter creation is also supported. To do this, use use <kbd>Shift</kbd>+<kbd>F</kbd> to enter live filter mode. Facet names are entered via their selector keybind, and `+` and `!` are entered normally. To confirm, use <kbd>Enter</kbd>, and to cancel, use <kbd>Esc</kbd>.

## Miscellaneous

The status message can be cleared and mode returned to default by pressing <kbd>Esc</kbd>. 
