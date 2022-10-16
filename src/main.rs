use linereader::LineReader;
use mimalloc::MiMalloc;
use naive_opt::SearchBytes;
use regex::bytes::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    env::{self, Args},
    fs::{self, File},
    io::{self, BufWriter, StdoutLock, Write},
    process::exit,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

macro_rules! error {
    ($format:literal$(, $args:expr)*) => {
        eprintln!($format$(, $args)*);
        print_help();
        exit(1);
    };
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
        let mut query = String::new();
        let mut filenames: Vec<String> = Vec::new();
        let mut max = 0;
        let mut flags: u8 = 0b00000000;
        let mut match_on = MatchOn::Anywhere;
        let mut finished = false;

        for arg in args.skip(1) {
            if !finished {
                if arg == "--" {
                    finished = true;
                    continue;
                } else if arg.starts_with('-') {
                    let trimmed = arg.trim_start_matches('-');
                    if let Some(stripped) = trimmed.strip_prefix("m=") {
                        max = stripped.parse().unwrap_or(0);
                        continue;
                    }
                    for c in trimmed.bytes() {
                        match c {
                            b'i' => flags |= 1 << 0,
                            b'n' => flags |= 1 << 1,
                            b'v' => flags |= 1 << 2,
                            b'F' => flags |= 1 << 3,
                            b'c' => flags |= 1 << 4,
                            b'w' => match_on = MatchOn::Word,
                            b'x' => match_on = MatchOn::Line,
                            b'h' => {
                                print_help();
                                exit(0);
                            }
                            _ => {
                                error!("Invalid flag: {}", c as char);
                            }
                        }
                    }
                    continue;
                }
            }

            if query.is_empty() {
                query = arg;
            } else if let Ok(md) = fs::metadata(&arg) {
                if md.is_file() {
                    filenames.push(arg);
                } else if md.is_dir() {
                    walk(&mut filenames, &arg);
                }
            }
            filenames.dedup();
        }

        if query.is_empty() {
            error!("No query specified");
        }

        // Toggle string search if the query contains no special characters
        // This is done because string search is faster than regex search
        if flags & (1 << 3) == 0 {
            let plain_text = regex::RegexBuilder::new(r"^[a-zA-Z0-9\s]").build().unwrap();
            if plain_text.is_match(&query) {
                flags |= 0b00001000;
            }
        }

        Self {
            query,
            filenames,
            max,
            flags,
            match_on,
        }
    }
}

fn walk(filenames: &mut Vec<String>, dir: &str) {
    let mut files = if let Ok(files) = fs::read_dir(dir) {
        files
    } else {
        return;
    };
    while let Some(Ok(f)) = files.next() {
        let metadata = if let Ok(metadata) = f.metadata() {
            metadata
        } else {
            continue;
        };

        if let Some(path) = f.path().to_str() {
            if metadata.is_file() {
                filenames.push(path.to_owned());
                continue;
            }

            if metadata.is_dir() {
                walk(filenames, path);
                continue;
            }
        }
    }
}

fn print_help() {
    println!(concat!(
        "Usage: grepox [OPTION]... QUERY [FILES]...\n",
        "Search for QUERY in FILES.\n",
        "Example:\n",
        "    grepox -i 'hello world' file1.txt file2.txt\n\n",
        "Options:\n",
        "-i          Ignore case distinctions in QUERY\n",
        "-n          Print line number with output lines\n",
        "-v          Invert match: select non-matching lines\n",
        "-F          String searching, disables regex\n",
        "-x          Only match whole lines, only works with -F\n",
        "-w          Only match whole words, only works with -F\n",
        "-m=<NUM>    Stop after NUM matches\n",
        "-h          Print this help and exit"
    ));
}

fn main() {
    grep(Config::new(env::args()));
}

fn grep(cfg: Config) {
    let multiple_files = cfg.filenames.len() > 1;

    let mut matches: u32 = 0;

    let query = cfg.query;
    let filenames = cfg.filenames;
    let max = cfg.max;
    let has_max = max > 0;

    let flags = cfg.flags;

    macro_rules! flag {
        ($pos:literal) => {
            flags & (1 << $pos) != 0
        };
    }
    let case_insensitive = flag!(0);
    let show_lines = flag!(1);
    let invert = flag!(2);
    let string_search = flag!(3);
    let color = flag!(4);

    let match_on = cfg.match_on;

    let istty = atty::is(atty::Stream::Stdin);

    let stdout = std::io::stdout();
    let stdout = stdout.lock();
    let mut writer = BufWriter::with_capacity(16384, stdout);

    if string_search {
        let query = if case_insensitive {
            query.to_lowercase()
        } else {
            query
        };
        let query = query.as_bytes();
        if !istty {
            let mut reader = LineReader::new(io::stdin());
            let mut i = 0;
            while let Some(Ok(line)) = reader.next_line() {
                if check_string(
                    &mut writer,
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
                    if matches >= max {
                        break;
                    }
                }

                i += 1;
            }
            return;
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        for filename in &filenames {
            let mut matches: u32 = 0;
            let reader = &mut read_file(filename);

            let mut i = 0;
            while let Some(Ok(line)) = reader.next_line() {
                if check_string(
                    &mut writer,
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
                    if matches >= max {
                        break;
                    }
                }

                i += 1;
            }
        }
    } else {
        let re = RegexBuilder::new(&query)
            .case_insensitive(case_insensitive)
            .build();

        if let Err(err) = &re {
            error!("Error parsing regex: {}", err);
        }

        let re = re.unwrap();

        if !istty {
            let mut reader = LineReader::new(io::stdin());
            let mut i = 0;
            while let Some(Ok(line)) = reader.next_line() {
                if check_regex(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    color,
                    i,
                    line,
                    "stdin",
                    &re,
                ) && has_max
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                i += 1;
            }
            return;
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        for filename in &filenames {
            let mut matches: u32 = 0;
            let reader = &mut read_file(filename);

            let mut i = 0;
            while let Some(Ok(line)) = reader.next_line() {
                if check_regex(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    color,
                    i,
                    line,
                    filename,
                    &re,
                ) && has_max
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                i += 1;
            }
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

fn read_file(filename: &str) -> LineReader<File> {
    File::open(filename).map_or_else(
        |_| {
            error!("Error reading {}", filename);
        },
        LineReader::new,
    )
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
            let mut found = false;
            let mut moved = 0;
            let mut colored = line.to_vec();
            let len = pattern.len();

            (&*line)
                .search_indices_bytes(pattern)
                .for_each(|(start, _)| {
                    found = true;
                    let index = start + moved;
                    colored.insert_bytes(index, b"\x1b[31;1m");
                    colored.insert_bytes(index + len + 7, b"\x1b[m");
                    moved += 10;
                });
            if !found {
                return false;
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
                    let _ = colored.write_all(word);
                    colored.push(b' ')
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
        let mut line = line.to_vec();
        let mut moved = 0;
        let mut found = false;
        let bytes = line.clone();
        for loc in pattern.find_iter(&bytes) {
            found = true;
            line.insert_bytes(loc.end() + moved, b"\x1b[31;1m");
            line.insert_bytes(loc.end() + 7 + moved, b"\x1b[m");
            moved += 10;
        }
        if !found {
            return false;
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

pub trait InsertBytes {
    fn insert_bytes(&mut self, idx: usize, bytes: &[u8]);
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
}
