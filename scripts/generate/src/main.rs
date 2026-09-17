//! Writes `src/Tables.mw` and `src/Cases.mw` from `unicode-width`.
//!
//! ```text
//! cargo run --release -- <package root>
//! ```
//!
//! Two kinds of output:
//!
//! * **Tables** -- the crate's own lookup tables, read from its `tables.rs` (see
//!   `build.rs`) and written as Meadow string literals. The hot-path trie is
//!   copied byte for byte; the predicates used only in rare contexts are written
//!   as sorted ranges, read off the crate's functions for every code point.
//! * **Cases** -- inputs with the widths the crate gives them: every string in
//!   the crate's `tests.rs`, every sequence in its `emoji-test.txt`, sequences
//!   built at random from the characters the state machine cares about, and the
//!   code points either side of every change in the tables. The expected values
//!   come from calling the crate, so the Meadow tests hold the port to it.
//!
//! The state machine itself is ported by hand into `src/Machine.mw`, since it
//! is code rather than data. Its source is fingerprinted: if a new version of
//! the crate changes it, this refuses to run until the port has been updated to
//! match and the fingerprint moved.

#[allow(dead_code, clippy::all)]
mod upstream {
    include!(concat!(env!("OUT_DIR"), "/upstream.rs"));
}

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The fingerprint of the upstream state machine that `src/Machine.mw` ports.
/// Printed by the check below when it differs.
const MACHINE: u64 = 0x7371_6444_acff_fb7f;

const SCALARS: u32 = 0x11_0000;

/// The crate version pinned in `Cargo.toml`.
const UPSTREAM_VERSION: &str = "0.2.2";

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "../..".into()));
    let upstream = PathBuf::from(env!("UPSTREAM_DIR"));

    check_machine();

    let tables = tables();
    let tables_path = root.join("src/Tables.mw");
    std::fs::write(&tables_path, &tables).unwrap();
    eprintln!("wrote {} ({} bytes)", tables_path.display(), tables.len());

    let cases = cases(&upstream);
    let cases_path = root.join("src/Cases.mw");
    std::fs::write(&cases_path, &cases).unwrap();
    eprintln!("wrote {} ({} bytes)", cases_path.display(), cases.len());
}

// --- the machine fingerprint ------------------------------------------------------

/// Refuse to go on if the upstream state machine is not the one ported.
///
/// What is fingerprinted is the code of `tables.rs` with its data taken out:
/// the tables themselves, the table sizes, the version number and the match
/// arms that name particular characters, all of which the generated tables
/// already carry. What is left is the logic `Machine.mw` has to agree with.
fn check_machine() {
    let text = include_str!(concat!(env!("OUT_DIR"), "/tables_orig.rs"));
    let print = fingerprint(&logic_of(text));
    if std::env::var_os("DUMP_LOGIC").is_some() {
        print!("{}", logic_of(text));
    }
    if print != MACHINE {
        eprintln!(
            "error: the unicode-width state machine is not the one src/Machine.mw ports.\n\
             Compare `src/tables.rs` in {} with the previous version, carry any change\n\
             into src/Machine.mw, then set MACHINE in scripts/generate/src/main.rs to\n\
             {print:#x}",
            env!("UPSTREAM_DIR")
        );
        std::process::exit(1);
    }
}

fn logic_of(text: &str) -> String {
    let mut out = String::new();
    let mut in_data = false;
    let mut in_ligature = false;
    for line in text.lines() {
        let t = line.trim_start();
        if in_data {
            if line.starts_with("]);") || line.starts_with("];") {
                in_data = false;
            }
            continue;
        }
        if t.starts_with("static ") || t.starts_with("pub static ") {
            // A one-line table ends on its own line.
            in_data = !(line.ends_with("];") || line.ends_with("]);"));
            continue;
        }
        if t.starts_with("fn is_ligature_transparent") {
            in_ligature = true;
        }
        if in_ligature && t.starts_with("matches!") {
            continue;
        }
        if in_ligature && line == "}" {
            in_ligature = false;
        }
        let data = t.starts_with("//")
            || t.starts_with("const WIDTH_MIDDLE_LEN")
            || t.starts_with("const WIDTH_LEAVES_LEN")
            || t.starts_with("pub const UNICODE_VERSION")
            // `'\u{5DC}' => (1, WidthInfo::HEBREW_LETTER_LAMED),`
            || (t.starts_with("'\\u{") && t.contains("=> ("))
            // `0x23 => &TEXT_PRESENTATION_LEAF_0,`
            || (t.starts_with("0x") && t.contains(" => "));
        if !data {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// FNV-1a: stable across builds, which `DefaultHasher` does not promise.
fn fingerprint(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// --- encoding -----------------------------------------------------------------------

/// A number as `digits` base-64 digits, most significant first, each digit the
/// character `'0' + d` -- so `'0'` to `'o'`, one contiguous run of ASCII that a
/// reader turns back into a number with a subtraction.
///
/// Of those 64 characters only `\` needs escaping in a string literal, which
/// [`long_literal`] does. `"` and `$` fall outside the run.
fn digits(out: &mut String, value: u64, digits: u32) {
    assert!(
        value < 1 << (6 * digits),
        "{value} does not fit in {digits} digits"
    );
    for k in (0..digits).rev() {
        out.push(char::from(b'0' + ((value >> (6 * k)) & 63) as u8));
    }
}

/// `text` as one Meadow string literal, broken with `\`-newline every `width`
/// characters so that no line of the generated file is unreadably long. Only
/// ASCII is written raw, so that the file is plain text; everything else is a
/// `\u{…}` escape.
///
/// A continuation drops the next line's leading whitespace, so a space that
/// would start a line is written `\x20`.
fn long_literal(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / width * 4 + 2);
    out.push('"');
    for (i, c) in text.chars().enumerate() {
        let line_start = i > 0 && i % width == 0;
        if line_start {
            out.push_str("\\\n    ");
        }
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            // `${` would open a hole.
            '$' => out.push_str("\\$"),
            ' ' if line_start => out.push_str("\\x20"),
            ' '..='~' => out.push(c),
            _ => {
                let _ = write!(out, "\\u{{{:X}}}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

// --- tables -----------------------------------------------------------------------

fn bytes_as_digits(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        digits(&mut s, u64::from(b), 2);
    }
    s
}

/// Code points `c` for which `pred` holds, as maximal runs.
fn runs(pred: impl Fn(char) -> bool) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for cp in 0..SCALARS {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        if !pred(c) {
            continue;
        }
        match out.last_mut() {
            // Contiguous, or separated only by the surrogates, which no string
            // can hold and so no lookup can ask about.
            Some((_, hi)) if *hi + 1 == cp || (*hi == 0xD7FF && cp == 0xE000) => *hi = cp,
            _ => out.push((cp, cp)),
        }
    }
    out
}

fn range_table(ranges: &[(u32, u32)]) -> String {
    let mut s = String::new();
    for &(lo, hi) in ranges {
        digits(&mut s, lo.into(), 4);
        digits(&mut s, hi.into(), 4);
    }
    s
}

/// The value the trie gives `cp`, before the special cases: 0, 1 or 2, or 3
/// for "look it up in the special cases".
fn trie(root: &[u8; 256], cp: u32) -> u8 {
    let cp = cp as usize;
    let t1 = root[cp >> 13];
    let t2 = upstream::WIDTH_MIDDLE.0[usize::from(t1)][cp >> 7 & 0x3F];
    let packed = upstream::WIDTH_LEAVES.0[usize::from(t2)][cp >> 2 & 0x1F];
    packed >> (2 * (cp & 0b11)) & 0b11
}

/// The code points the trie sends to the special cases, with what the crate
/// answers for each: `(lo, hi, width, info)`, maximal runs of equal answers.
fn specials(
    root: &[u8; 256],
    lookup: impl Fn(char) -> (u8, upstream::WidthInfo),
) -> Vec<(u32, u32, u8, u16)> {
    let mut out: Vec<(u32, u32, u8, u16)> = Vec::new();
    for cp in 0..SCALARS {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        if trie(root, cp) != 3 {
            continue;
        }
        let (w, info) = lookup(c);
        match out.last_mut() {
            Some((_, hi, lw, li)) if *hi + 1 == cp && *lw == w && *li == info.0 => *hi = cp,
            _ => out.push((cp, cp, w, info.0)),
        }
    }
    out
}

fn special_table(specials: &[(u32, u32, u8, u16)]) -> String {
    let mut s = String::new();
    for &(lo, hi, w, info) in specials {
        digits(&mut s, lo.into(), 4);
        digits(&mut s, hi.into(), 4);
        digits(&mut s, w.into(), 1);
        digits(&mut s, info.into(), 3);
    }
    s
}

/// The 3-byte little-endian ranges the crate stores two of its tables as.
fn le3_ranges(table: &[([u8; 3], [u8; 3])]) -> Vec<(u32, u32)> {
    table
        .iter()
        .map(|(lo, hi)| {
            (
                u32::from_le_bytes([lo[0], lo[1], lo[2], 0]),
                u32::from_le_bytes([hi[0], hi[1], hi[2], 0]),
            )
        })
        .collect()
}

fn tables() -> String {
    use upstream::*;
    let (maj, min, pat) = UNICODE_VERSION;

    let middle: Vec<u8> = WIDTH_MIDDLE.0.iter().flatten().copied().collect();
    let leaves: Vec<u8> = WIDTH_LEAVES.0.iter().flatten().copied().collect();

    let special = specials(&WIDTH_ROOT.0, lookup_width);
    let special_cjk = specials(&WIDTH_ROOT_CJK.0, lookup_width_cjk);

    let emoji_presentation = runs(starts_emoji_presentation_seq);
    let text_presentation = runs(starts_non_ideographic_text_presentation_seq);
    let modifier_base = runs(is_emoji_modifier_base);
    let ligature_transparent = runs(is_ligature_transparent);
    let non_transparent = le3_ranges(&NON_TRANSPARENT_ZERO_WIDTHS);
    let solidus = le3_ranges(&SOLIDUS_TRANSPARENT);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "-- GENERATED by scripts/generate.sh from unicode-width {}.
-- Do not edit: run the script again instead.
--
-- Every table is a string of fixed-width records, each field written in the
-- base-64 digits '0' ('0' + 0) to 'o' ('0' + 63), most significant first. A
-- lookup reads the digits it needs with `stringByteAt`, which takes constant
-- time; see `Lookup.mw`.
--
-- Copyright 2012-2025 The Rust Project Developers, and the Meadow port's
-- authors. Dual-licensed under Apache-2.0 or MIT: see COPYRIGHT.
",
        UPSTREAM_VERSION
    );
    let _ = writeln!(
        out,
        "-- The Unicode version the tables describe.\n@pub(pkg) def unicodeVersion = ({maj}, {min}, {pat})\n"
    );

    let mut def = |name: &str, doc: &str, body: &str| {
        let _ = writeln!(
            out,
            "{doc}\n@pub(pkg) def {name} =\n  {}\n",
            long_literal(body, 96)
        );
    };
    def(
        "widthRoot",
        "-- The trie's first level, indexed by `cp >> 13`: 256 bytes, two digits each.",
        &bytes_as_digits(&WIDTH_ROOT.0),
    );
    def(
        "widthRootCjk",
        "-- The same, for East Asian contexts, where ambiguous characters are wide.",
        &bytes_as_digits(&WIDTH_ROOT_CJK.0),
    );
    def(
        "widthMiddle",
        "-- The second level: 64 bytes a block, indexed by `cp >> 7 & 63`.",
        &bytes_as_digits(&middle),
    );
    def(
        "widthLeaves",
        "-- The third: 32 bytes a block, indexed by `cp >> 2 & 31`, four 2-bit\n-- widths packed in each byte.",
        &bytes_as_digits(&leaves),
    );
    def(
        "specials",
        "-- Where the trie says 3: `lo hi` (4 digits each), then the width (1) and\n-- the state-machine information (3).",
        &special_table(&special),
    );
    def(
        "specialsCjk",
        "-- The same, for East Asian contexts.",
        &special_table(&special_cjk),
    );
    let ranges = [
        (
            "emojiPresentation",
            "starts an emoji presentation sequence when followed by U+FE0F",
            &emoji_presentation,
        ),
        (
            "textPresentation",
            "is emoji by default but not ideographic, and starts a text presentation\n-- sequence when followed by U+FE0E",
            &text_presentation,
        ),
        (
            "emojiModifierBase",
            "is an Emoji_Modifier_Base",
            &modifier_base,
        ),
        (
            "ligatureTransparent",
            "is a default-ignorable combining mark or ZWJ, which does not interrupt a\n-- non-Arabic ligature",
            &ligature_transparent,
        ),
        (
            "nonTransparentZeroWidths",
            "is zero-width but not Joining_Type=Transparent",
            &non_transparent,
        ),
        (
            "solidusTransparent",
            "does not stop U+0338 COMBINING LONG SOLIDUS OVERLAY acting on its base",
            &solidus,
        ),
    ];
    for (name, what, table) in ranges {
        def(
            name,
            &format!("-- Code points that {what}: `lo hi`, 4 digits each."),
            &range_table(table),
        );
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

// --- cases --------------------------------------------------------------------------

/// A small deterministic generator, so that the cases are the same on every run
/// and a change in them is a change in the crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The characters the state machine treats specially, and some it does not, so
/// that random sequences of them reach every branch.
const INTERESTING: &[u32] = &[
    // line breaks and ASCII
    0x0A, 0x0D, 0x20, 0x23, 0x2A, 0x30, 0x39, 0x3C, 0x3D, 0x3E, 0x61, 0x7F, 0xAD,
    // joiners and selectors
    0x200D, 0x200B, 0x34F, 0x180B, 0x180F, 0x17B4, 0xFE00, 0xFE01, 0xFE02, 0xFE0E, 0xFE0F, 0xE0100,
    0x20E3, // quotes that VS1-3 widen
    0x2018, 0x2019, 0x201C, 0x201D, // emoji, modifiers, regional indicators, tags
    0x1F468, 0x1F469, 0x1F466, 0x2764, 0x263A, 0x261D, 0x1F44D, 0x1F3FB, 0x1F3FF, 0x1F1E6, 0x1F1FA,
    0x1F1F8, 0x1F1FF, 0x1F3F4, 0xE0030, 0xE0039, 0xE0061, 0xE0067, 0xE007A, 0xE007F, 0x1F9D1,
    0x1F52C, 0x2B50, 0x231A, // Arabic: alef, lam and a transparent mark
    0x622, 0x627, 0x644, 0x6B5, 0x6B8, 0x76A, 0x8A6, 0x8C7, 0x610, 0x64B, 0x670, 0x6DD,
    // solidus overlay
    0x338, 0x301, 0x300,
    // Hebrew, Khmer, Buginese, Tifinagh, Lisu, Old Turkic, Kirat Rai
    0x5D0, 0x5DC, 0x1780, 0x17AF, 0x17D2, 0x17D8, 0x1A10, 0x1A15, 0x1A17, 0x2D31, 0x2D65, 0x2D6F,
    0x2D7F, 0xA4F8, 0xA4FB, 0xA4FC, 0xA4FD, 0x10C03, 0x10C32, 0x16D63, 0x16D67, 0x16D68, 0x16D69,
    // wide and ambiguous, Hangul jamo
    0x4E00, 0xFF48, 0x2081, 0x3000, 0x1100, 0x1160, 0x11A8, 0xAC00, 0xD7B0,
    // Devanagari
    0x915, 0x94D, 0x937,
];

/// Every string literal in `source` -- Rust's, with its escapes -- that is not
/// a byte string or a raw one.
fn string_literals(source: &str) -> (Vec<String>, Vec<char>) {
    let mut strings = Vec::new();
    let mut chars = Vec::new();
    let s: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            '/' if s.get(i + 1) == Some(&'/') => {
                while i < s.len() && s[i] != '\n' {
                    i += 1;
                }
            }
            '"' if i == 0 || !matches!(s[i - 1], 'b' | 'r' | '#') => {
                let (text, end) = read_literal(&s, i + 1, '"');
                strings.push(text);
                i = end;
            }
            '\'' => {
                // A char literal is one character, or one escape, then `'`;
                // anything else is a lifetime.
                let (text, end) = read_literal(&s, i + 1, '\'');
                let mut cs = text.chars();
                if let (Some(c), None) = (cs.next(), cs.next())
                    && s.get(end - 1) == Some(&'\'')
                {
                    chars.push(c);
                    i = end;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    (strings, chars)
}

/// The literal starting at `i`, up to the unescaped `close`, and where it ends.
fn read_literal(s: &[char], mut i: usize, close: char) -> (String, usize) {
    let mut out = String::new();
    while i < s.len() {
        let c = s[i];
        if c == close {
            return (out, i + 1);
        }
        if close == '\'' && (c == '\n' || out.chars().count() > 1) {
            return (String::new(), i);
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let e = s.get(i + 1).copied().unwrap_or(' ');
        i += 2;
        match e {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            '\\' | '"' | '\'' => out.push(e),
            'x' => {
                let hex: String = s[i..i + 2].iter().collect();
                out.push(char::from(u8::from_str_radix(&hex, 16).unwrap()));
                i += 2;
            }
            'u' => {
                let end = i + s[i..].iter().position(|&c| c == '}').unwrap();
                let hex: String = s[i + 1..end].iter().collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
                i = end + 1;
            }
            '\n' => {
                while i < s.len() && s[i].is_whitespace() {
                    i += 1;
                }
            }
            _ => panic!("unknown escape \\{e}"),
        }
    }
    (out, i)
}

fn cases(upstream: &Path) -> String {
    let mut strings: Vec<String> = Vec::new();

    // The crate's own tests.
    let tests = std::fs::read_to_string(upstream.join("tests/tests.rs")).unwrap();
    let (literals, test_chars) = string_literals(&tests);
    strings.extend(literals);
    strings.extend(test_chars.iter().map(|c| c.to_string()));

    // Every sequence in the emoji test file, qualified or not.
    let emoji = std::fs::read_to_string(upstream.join("tests/emoji-test.txt")).unwrap();
    for line in emoji.lines() {
        let Some((codes, _)) = line.split_once(';') else {
            continue;
        };
        if line.starts_with('#') {
            continue;
        }
        let seq: Option<String> = codes
            .split_whitespace()
            .map(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
            .collect();
        if let Some(seq) = seq {
            strings.push(seq);
        }
    }

    // Random sequences over the characters the machine cares about.
    let mut rng = Rng(0x5eed_ba11_c0ff_ee42);
    for _ in 0..6000 {
        let len = 1 + rng.below(8);
        let seq: String = (0..len)
            .map(|_| char::from_u32(INTERESTING[rng.below(INTERESTING.len())]).unwrap())
            .collect();
        strings.push(seq);
    }

    strings.sort();
    strings.dedup();

    let mut str_cases = String::new();
    for s in &strings {
        digits(&mut str_cases, s.width() as u64, 2);
        digits(&mut str_cases, s.width_cjk() as u64, 2);
        digits(&mut str_cases, s.len() as u64, 3);
        str_cases.push_str(s);
    }

    // Single characters: either side of every change in the trie, every
    // special case, and the characters the tests name.
    let mut cps: Vec<u32> = Vec::new();
    let mut last: Option<(Option<usize>, Option<usize>)> = None;
    for cp in 0..SCALARS {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let now = (c.width(), c.width_cjk());
        if last != Some(now) {
            cps.extend([cp.saturating_sub(1), cp]);
        }
        last = Some(now);
    }
    cps.extend(INTERESTING);
    cps.extend(test_chars.iter().map(|&c| u32::from(c)));
    cps.push(SCALARS - 1);
    cps.sort_unstable();
    cps.dedup();
    let mut char_cases = String::new();
    for cp in cps {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let code = |w: Option<usize>| w.map_or(0, |w| w as u64 + 1);
        digits(&mut char_cases, cp.into(), 4);
        digits(&mut char_cases, code(c.width()), 1);
        digits(&mut char_cases, code(c.width_cjk()), 1);
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "-- GENERATED by scripts/generate.sh from unicode-width {}.
-- Do not edit: run the script again instead.
--
-- Inputs, with the widths the crate gives them, for the tests in `Tests.mw`.
-- {} strings and {} characters, some from the crate's own tests.
--
-- Copyright 2012-2025 The Rust Project Developers, and the Meadow port's
-- authors. Dual-licensed under Apache-2.0 or MIT: see COPYRIGHT.
",
        UPSTREAM_VERSION,
        strings.len(),
        char_cases.len() / 6
    );
    let _ = writeln!(
        out,
        "-- Each: the width (2 digits), the CJK width (2), the length in bytes (3),\n\
         -- then that many bytes of the string itself.\n\
         @cfg(test)\n@pub(pkg) def strCases =\n  {}\n",
        long_literal(&str_cases, 96)
    );
    let _ = writeln!(
        out,
        "-- Each: the code point (4 digits), then its width and its CJK width, one\n\
         -- digit each: 0 for none, otherwise the width plus one.\n\
         @cfg(test)\n@pub(pkg) def charCases =\n  {}",
        long_literal(&char_cases, 96)
    );
    out
}
