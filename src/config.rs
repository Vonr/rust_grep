use regex_automata::dfa::{dense, Automaton};

use crate::{error, print_help, MatchOn};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[allow(clippy::upper_case_acronyms)]
type DFA = dense::DFA<&'static [S]>;

// Adapted from regex-automata docs (https://github.com/BurntSushi/regex-automata/blob/0ba880134d649866fa15809dec9c6eae89cd7591/UNLICENSE):
// https://docs.rs/regex-automata/latest/regex_automata/dfa/dense/struct.DFA.html#method.from_bytes
type S = u32;

#[repr(C)]
struct Aligned<B: ?Sized> {
    _align: [S; 0],
    bytes: B,
}

const ALIGNED: &Aligned<[u8]> = &Aligned {
    _align: [],
    #[cfg(target_endian = "big")]
    bytes: *include_bytes!(concat!(env!("OUT_DIR"), "/plaintext_regex_be")),
    #[cfg(target_endian = "little")]
    bytes: *include_bytes!(concat!(env!("OUT_DIR"), "/plaintext_regex_le")),
};

enum ConfigState {
    Flag,
    End,
    Invalid,
    WantsMax,
    Max(bool),
    Space,
}

pub struct ConfigParser {
    state: ConfigState,
    flags: Flags,
    max: u32,
    match_on: MatchOn,
}

impl ConfigParser {
    #[inline]
    pub fn new() -> Self {
        Self {
            state: ConfigState::Space,
            flags: Flags::default(),
            max: 0,
            match_on: MatchOn::Anywhere,
        }
    }

    #[inline]
    pub fn tick(&mut self, byte: u8) {
        match self.state {
            ConfigState::End => (),
            ConfigState::Invalid => error!("Invalid state"),
            ConfigState::Flag => match byte {
                b'-' => self.state = ConfigState::End,
                b'i' => self.flags.case_insensitive = true,
                b'n' => self.flags.show_lines = true,
                b'v' => self.flags.invert = true,
                b'F' => self.flags.string_search = true,
                b'c' => self.flags.color = true,
                b'U' => self.flags.no_unicode = true,
                b'q' => self.flags.quiet = true,
                b'w' => self.match_on = MatchOn::Word,
                b'x' => self.match_on = MatchOn::Line,
                b'm' => self.state = ConfigState::WantsMax,
                b'h' => print_help!(),
                b' ' => self.state = ConfigState::Space,
                _ => self.state = ConfigState::Invalid,
            },
            ConfigState::WantsMax => match byte {
                b'=' | b' ' => {
                    self.state = ConfigState::Max(false);
                    self.max = 0;
                }
                _ => self.state = ConfigState::Invalid,
            },
            ConfigState::Max(found) => match byte {
                b'0'..=b'9' => {
                    self.state = ConfigState::Max(true);
                    self.max = self.max * 10 + (byte - b'0') as u32
                }
                b' ' => {
                    if found {
                        self.state = ConfigState::Space
                    }
                }
                _ => self.state = ConfigState::Invalid,
            },
            ConfigState::Space => match byte {
                b'-' => self.state = ConfigState::Flag,
                b' ' => (),
                _ => self.state = ConfigState::End,
            },
        }
    }

    #[inline]
    pub fn run(&mut self, tape: &[u8]) -> bool {
        for c in tape {
            self.tick(*c);
            if matches!(self.state, ConfigState::End) {
                return false;
            }
        }
        self.tick(b' ');
        true
    }
}

#[derive(Default)]
#[repr(align(8))]
pub struct Flags {
    pub case_insensitive: bool,
    pub show_lines: bool,
    pub invert: bool,
    pub string_search: bool,
    pub color: bool,
    pub no_unicode: bool,
    pub quiet: bool,
    pub multiple_files: bool,
}

pub struct Config {
    pub query: String,
    pub filenames: Vec<PathBuf>,
    pub max: u32,
    pub flags: Flags,
    pub(crate) match_on: MatchOn,
}

impl Config {
    pub fn new() -> Self {
        let mut filenames = Vec::new();
        let mut parser = ConfigParser::new();

        let mut args = std::env::args()
            .skip(1)
            .skip_while(|arg| parser.run(arg.as_bytes()));

        let query = args.next().unwrap_or_else(|| error!("No query specified"));

        let mut has_dir = false;
        for arg in args {
            if let Ok(md) = fs::metadata(&arg) {
                if md.is_file() {
                    filenames.push(arg.into());
                } else if md.is_dir() {
                    has_dir = true;
                    walk(&mut filenames, &arg);
                }
            }
        }

        parser.flags.multiple_files |= has_dir;

        // Toggle string search if the query contains no special characters
        // This is done because string search is faster than regex search
        if !parser.flags.string_search {
            let plain_text = DFA::from_bytes(&ALIGNED.bytes).unwrap().0;
            if plain_text
                .try_search_fwd(&query.as_bytes().into())
                .map(|m| m.is_none())
                .unwrap_or(false)
            {
                parser.flags.string_search = true;
            }
        }

        Self {
            query,
            filenames,
            max: parser.max,
            flags: parser.flags,
            match_on: parser.match_on,
        }
    }
}

fn walk<P: AsRef<Path>>(filenames: &mut Vec<PathBuf>, dir: P) {
    if let Ok(files) = fs::read_dir(dir) {
        files.filter_map(Result::ok).for_each(|f| {
            let file_type = if let Ok(file_type) = f.file_type() {
                file_type
            } else {
                return;
            };

            if file_type.is_file() {
                let path = f.path();
                if !filenames.contains(&path) {
                    filenames.push(path);
                }
            } else if file_type.is_dir() {
                walk(filenames, f.path());
            }
        });
    }
}
