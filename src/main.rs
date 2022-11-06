use bstr::io::BufReadExt;
use mimalloc::MiMalloc;
use naive_opt::SearchBytes;
use regex::bytes::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    env::{self, Args},
    fs::{self, File},
    io::{self, BufWriter, Read, StdoutLock, Write},
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

macro_rules! print_help {
    () => {{
        println!(
            r"Usage: grepox [OPTION]... QUERY [FILES]...
Search for QUERY in FILES.
Example:
    # Finds the phrase 'hello world' case-insensitively in file1.txt
    # and file2.txt and prints matches in color
    grepox -ci 'hello world' file1.txt file2.txt

Options:
-i          Ignore case distinctions in QUERY
-n          Print line number with output lines
-v          Invert match: select non-matching lines
-F          String searching, disables regex
-x          Only match whole lines, only works with -F
-w          Only match whole words, only works with -F
-m=<NUM>    Stop after NUM matches
-c          Colorizes output
-h          Print this help and exit"
        );
        ::std::process::exit(1)
    }};
}

macro_rules! error {
    ($format:literal$(, $args:expr)*) => {{
        eprintln!($format$(, $args)*);
        print_help!();
    }};
}

enum ConfigState {
    Flag,
    End,
    Invalid,
    WantsMax,
    Max(bool),
    Space,
}

struct ConfigParser {
    state: ConfigState,
    flags: u8,
    max: u32,
    match_on: MatchOn,
}

impl ConfigParser {
    pub const fn new() -> Self {
        Self {
            state: ConfigState::Space,
            flags: 0,
            max: 0,
            match_on: MatchOn::Anywhere,
        }
    }

    pub fn tick(&mut self, byte: u8) {
        match self.state {
            ConfigState::End => (),
            ConfigState::Invalid => error!("Invalid state"),
            ConfigState::Flag => match byte {
                b'-' => self.state = ConfigState::End,
                b'i' => self.flags |= 1 << 0,
                b'n' => self.flags |= 1 << 1,
                b'v' => self.flags |= 1 << 2,
                b'F' => self.flags |= 1 << 3,
                b'c' => self.flags |= 1 << 4,
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

    pub fn run(&mut self, tape: &[u8]) -> bool {
        for c in tape {
            self.tick(*c);
            if matches!(self.state, ConfigState::End) {
                return false;
            }
        }
        self.tick(b' ');
        if matches!(self.state, ConfigState::End) {
            return false;
        }
        true
    }
}

struct Config {
    pub query: String,
    pub filenames: Vec<String>,
    pub max: u32,
    pub flags: u8,
    pub match_on: MatchOn,
}

#[derive(PartialEq, Clone, Copy)]
enum MatchOn {
    Anywhere,
    Line,
    Word,
}

impl Config {
    fn new(args: Args) -> Self {
        let mut filenames: Vec<String> = Vec::new();
        let mut parser = ConfigParser::new();

        let mut args = args.skip(1).skip_while(|arg| parser.run(arg.as_bytes()));

        let query = args.next().unwrap_or_else(|| error!("No query specified"));

        let mut has_dir = false;
        args.for_each(|arg| {
            if let Ok(md) = fs::metadata(&arg) {
                if md.is_file() {
                    filenames.push(arg);
                } else if md.is_dir() {
                    has_dir = true;
                    walk(&mut filenames, &arg);
                }
            }
        });
        parser.flags |= (has_dir as u8) << 5;

        // Toggle string search if the query contains no special characters
        // This is done because string search is faster than regex search
        if parser.flags & (1 << 3) == 0 {
            let plain_text = regex::RegexBuilder::new(r#"[^[:alnum:] ;:~!@#%&\-_='",<>/]"#)
                .build()
                .unwrap();
            if !plain_text.is_match(&query) {
                parser.flags |= 1 << 3;
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

fn walk(filenames: &mut Vec<String>, dir: &str) {
    if let Ok(files) = fs::read_dir(dir) {
        files.filter_map(|f| f.ok()).for_each(|f| {
            if let Some(path) = f.path().to_str() {
                let metadata = if let Ok(metadata) = f.metadata() {
                    metadata
                } else {
                    return;
                };
                if metadata.is_file() {
                    let path = path.to_owned();
                    if !filenames.contains(&path) {
                        filenames.push(path);
                    }
                }

                if metadata.is_dir() {
                    walk(filenames, path);
                }
            }
        });
    }
}

fn grep(cfg: Config) {
    let flags = cfg.flags;
    macro_rules! flag {
        ($pos:literal) => {{
            flags & (1 << $pos) != 0
        }};
    }

    let multiple_files = flag!(5) || cfg.filenames.len() > 1;

    let mut matches: u32 = 0;

    let query = cfg.query;
    let filenames = cfg.filenames;
    let max = cfg.max;
    let has_max = max > 0;

    let case_insensitive = flag!(0);
    let show_lines = flag!(1);
    let invert = flag!(2);
    let string_search = flag!(3);
    let color = flag!(4);

    let match_on = cfg.match_on;

    let is_tty = atty::is(atty::Stream::Stdin);

    let stdout = std::io::stdout();
    let stdout = stdout.lock();
    let mut writer = BufWriter::with_capacity(16384, stdout);

    if string_search {
        let query = if case_insensitive {
            query.to_lowercase()
        } else {
            query
        };
        let query = &query.into_bytes();
        let writer = &mut writer;
        if !is_tty {
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            let mut i = 0;
            let _ = stdin.for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_string(
                    writer,
                    show_lines,
                    multiple_files,
                    invert,
                    case_insensitive,
                    color,
                    match_on,
                    i,
                    line,
                    "stdin",
                    query,
                ) && has_max
                {
                    matches += 1;
                }

                i += 1;
                Ok(true)
            });
            return;
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        for filename in &filenames {
            let mut matches: u32 = 0;
            let mut reader: &[u8] = &read_file(filename);
            let mut i = 0;

            let _ = reader.for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_string(
                    writer,
                    show_lines,
                    multiple_files,
                    invert,
                    case_insensitive,
                    color,
                    match_on,
                    i,
                    line,
                    filename,
                    query,
                ) && has_max
                {
                    matches += 1;
                }

                i += 1;
                Ok(true)
            });
        }
    } else {
        let writer = &mut writer;
        let re = RegexBuilder::new(&query)
            .case_insensitive(case_insensitive)
            .multi_line(true)
            .build();

        let re = &match re {
            Err(err) => error!("Error parsing regex: {}", err),
            Ok(re) => re,
        };

        if !is_tty {
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            let mut i = 0;
            let _ = stdin.for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_regex(
                    writer,
                    show_lines,
                    multiple_files,
                    invert,
                    color,
                    i,
                    line,
                    "stdin",
                    re,
                ) && has_max
                {
                    matches += 1;
                }

                i += 1;
                Ok(true)
            });
            return;
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        for filename in &filenames {
            let mut matches: u32 = 0;
            let mut reader: &[u8] = &read_file(filename);

            let mut i = 0;
            let _ = reader.for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_regex(
                    writer,
                    show_lines,
                    multiple_files,
                    invert,
                    color,
                    i,
                    line,
                    filename,
                    re,
                ) && has_max
                {
                    matches += 1;
                }

                i += 1;
                Ok(true)
            });
        }
    }
    writer.flush().unwrap();
}

fn print_match(
    writer: &mut BufWriter<StdoutLock>,
    index: usize,
    line: &[u8],
    show_lines: bool,
    filename: &str,
    multiple_files: bool,
) {
    let res = if multiple_files {
        if show_lines {
            write!(writer, "{}:{}:", filename, index + 1)
        } else {
            write!(writer, "{}:", filename)
        }
    } else if show_lines {
        write!(writer, "{}:", index + 1)
    } else {
        Ok(())
    };
    if let Err(e) = res.and_then(|_| writer.write_all(line)) {
        error!("Error writing to stdout: {}", e);
    }
}

fn read_file(filename: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = File::open(filename)
        .unwrap_or_else(|e| error!("Error reading file {}: {}", filename, e))
        .read_to_end(&mut buf)
        .map_err(|e| error!("Error reading file {}: {}", filename, e));
    buf
}

#[allow(clippy::too_many_arguments)]
fn check_string(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    case_insensitive: bool,
    color: bool,
    match_on: MatchOn,
    i: usize,
    line: &[u8],
    source: &str,
    pattern: &[u8],
) -> bool {
    let line = if case_insensitive {
        Cow::Owned(line.to_ascii_lowercase())
    } else {
        Cow::Borrowed(line)
    };
    if !color || invert || match_on == MatchOn::Line {
        match match_on {
            MatchOn::Anywhere => {
                if !(&*line).includes_bytes(pattern) ^ invert {
                    return false;
                }
            }
            MatchOn::Line => {
                if (line != pattern) ^ invert {
                    return false;
                }
            }
            MatchOn::Word => {
                if line
                    .split(|c| match c {
                        b' ' | b'\x09'..=b'\x0d' => true,
                        c => *c > b'\x7f',
                    })
                    .all(|word| (word != pattern) ^ invert)
                {
                    return false;
                }
            }
        }

        print_match(writer, i, &line, show_lines, source, multiple_files);
        return true;
    }

    let line = match match_on {
        MatchOn::Anywhere => {
            let indices: Vec<usize> = (&*line)
                .search_indices_bytes(pattern)
                .map(|(start, _)| start)
                .collect();
            if indices.is_empty() {
                return false;
            }

            let mut colored = {
                let mut _colored = Vec::with_capacity(line.len() + indices.len() * 10);
                _colored.extend_from_slice(&line);
                _colored
            };

            let mut moved = 0;
            let len = pattern.len();

            unsafe {
                indices.into_iter().for_each(|idx| {
                    colored.insert_bytes_unchecked(idx + moved, b"\x1b[31;1m");
                    colored.insert_bytes_unchecked(idx + moved + len + 7, b"\x1b[m");
                    moved += 10;
                });
            }

            colored
        }
        MatchOn::Word => {
            let mut colored = Vec::new();
            let mut found = false;
            line.split(|c| match c {
                b' ' | b'\x09'..=b'\x0d' => true,
                c => *c > b'\x7f',
            })
            .for_each(|word| {
                if word == pattern {
                    found = true;
                    let _ = colored
                        .write_all(b"\x1b[31;1m")
                        .and_then(|_| colored.write_all(word))
                        .and_then(|_| colored.write_all(b"\x1b[m "));
                } else {
                    let _ = colored.write_all(word).map(|_| colored.push(b' '));
                }
            });
            if found {
                colored.push(b'\n');
                colored
            } else {
                return false;
            }
        }
        MatchOn::Line => {
            unreachable!()
        }
    };
    print_match(writer, i, &line, show_lines, source, multiple_files);
    true
}

#[allow(clippy::too_many_arguments)]
fn check_regex(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    color: bool,
    i: usize,
    line: &[u8],
    source: &str,
    pattern: &Regex,
) -> bool {
    if color && !invert {
        let indices: Vec<(usize, usize)> = pattern
            .find_iter(line)
            .map(|loc| (loc.start(), loc.end()))
            .collect();
        if indices.is_empty() {
            return false;
        }

        let mut line = {
            let mut _line = Vec::with_capacity(line.len() + indices.len() * 10);
            _line.extend_from_slice(line);
            _line
        };

        let mut moved = 0;
        unsafe {
            indices.into_iter().for_each(|(start, end)| {
                line.insert_bytes_unchecked(start + moved, b"\x1b[31;1m");
                line.insert_bytes_unchecked(end + moved + 7, b"\x1b[m");
                moved += 10;
            });
        }
        print_match(writer, i, &line, show_lines, source, multiple_files);
        return true;
    }
    if pattern.is_match(line) ^ invert {
        print_match(writer, i, line, show_lines, source, multiple_files);
        return true;
    }
    false
}

trait InsertBytes {
    fn insert_bytes(&mut self, idx: usize, bytes: &[u8]);
    /// # Safety
    ///
    /// This function requires the caller to uphold the safety contract where the Vec's capacity is
    /// over the sum of its current length and the length of the bytes to be inserted.
    unsafe fn insert_bytes_unchecked(&mut self, idx: usize, bytes: &[u8]);
}

impl InsertBytes for Vec<u8> {
    fn insert_bytes(&mut self, idx: usize, bytes: &[u8]) {
        let len = self.len();
        let amt = bytes.len();
        self.reserve(amt);

        unsafe {
            std::ptr::copy(
                self.as_ptr().add(idx),
                self.as_mut_ptr().add(idx + amt),
                len - idx,
            );
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.as_mut_ptr().add(idx), amt);
            self.set_len(len + amt);
        }
    }

    unsafe fn insert_bytes_unchecked(&mut self, idx: usize, bytes: &[u8]) {
        let len = self.len();
        let amt = bytes.len();

        unsafe {
            std::ptr::copy(
                self.as_ptr().add(idx),
                self.as_mut_ptr().add(idx + amt),
                len - idx,
            );
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.as_mut_ptr().add(idx), amt);
            self.set_len(len + amt);
        }
    }
}

fn main() {
    grep(Config::new(env::args()));
}
