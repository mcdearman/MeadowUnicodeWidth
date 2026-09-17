# unicodeWidth

How many columns a character or a string takes up in a terminal, for
[Meadow](https://github.com/mcdearman/meadow).

This is a port of Rust's [`unicode-width`](https://github.com/unicode-rs/unicode-width)
0.2.2, covering Unicode 17.0.0. It follows
[UAX #11](https://www.unicode.org/reports/tr11/) plus the crate's own rules for
the cases UAX #11 leaves open: emoji ZWJ sequences, flags, keycaps, variation
selectors, `\r\n`, Arabic lam-alef ligatures, and others.

## Install

```sh
meadow add mcdearman/meadow-unicode-width
```

## Use

```meadow
use unicodeWidth (width, widthCjk, charWidth, charWidthCjk)

def main =
  let a = width "hello" in              -- 5
  let b = width "日本語" in              -- 6
  let c = width "👨‍👩‍👧" in                -- 2: one emoji, five code points
  let d = charWidth '\u{7}' in          -- None: a control character
  let e = (charWidth '○', charWidthCjk '○') in   -- (Just 1, Just 2)
  (a, b, c, d, e)
```

| function | type | |
|---|---|---|
| `width` | `String -> Int` | columns taken by a string |
| `widthCjk` | `String -> Int` | the same, with ambiguous-width characters counted as wide |
| `charWidth` | `Char -> Maybe Int` | columns taken by one character, or `None` for a control character |
| `charWidthCjk` | `Char -> Maybe Int` | the same, with ambiguous-width characters counted as wide |
| `unicodeVersion` | `(Int, Int, Int)` | `(17, 0, 0)` |

Use the `Cjk` versions for East Asian text. Characters with
`East_Asian_Width=Ambiguous` (`○`, `§`, `→`, …) take two columns there and one
column everywhere else.

A string's width is not always the sum of its characters' widths, so for text
use `width` rather than adding up `charWidth`s. Inside a string, control
characters count as one column each, as in the crate.

These are the widths a terminal is *likely* to use. No standard requires
terminals to use them, and some don't.

## How it's made

- **`src/Tables.mw`** is generated from the crate's own lookup tables. The
  three-level trie is copied byte for byte, so looking up a character takes
  constant time. The data is stored as base-64 string literals that are read in
  place and never unpacked.
- **`src/Machine.mw`** is a hand translation of the crate's `width_in_str` and
  `width_in_str_cjk`, arm for arm.
- **`src/Cases.mw`** is generated test data: every string in the crate's test
  suite, every sequence in `emoji-test.txt`, 6,000 random sequences of the
  characters the state machine treats specially, and the characters on each
  side of every width change. That comes to 10,799 strings and 2,260
  characters. The expected widths come from calling the crate itself, and
  `meadow test` checks that this port gives the same answer for every one.

To regenerate, or to move to a newer version of the crate, update the version
pin in `scripts/generate/Cargo.toml` and run:

```sh
scripts/generate.sh
```

This needs a Rust toolchain. The generator fingerprints the crate's state
machine and refuses to run if it changed, because then `src/Machine.mw` has to
be updated by hand first.

## Licence

Like the crate, this package is dual-licensed under [Apache-2.0](LICENSE-APACHE)
or [MIT](LICENSE-MIT), at your option. See [COPYRIGHT](COPYRIGHT).
