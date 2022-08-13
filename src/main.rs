use linereader::LineReader;
use naive_opt::SearchBytes;
use regex::bytes::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    env::{self, Args},
    fs::File,
    io::{self, BufWriter, StdoutLock, Write},
    process::exit,
};

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
                            b'i' => flags |= 0b00000001,
                            b'n' => flags |= 0b00000010,
                            b'v' => flags |= 0b00000100,
                            b'F' => flags |= 0b00001000,
                            b'w' => match_on = MatchOn::Word,
                            b'x' => match_on = MatchOn::Line,
                            b'h' => {
                                print_help();
                                exit(0);
                            }
                            _ => {
                                error(&format!("Invalid flag: {}", c));
                            }
                        }
                    }
                    continue;
                }
            }

            if query.is_empty() {
                query = arg;
            } else {
                filenames.push(arg);
            }
            filenames.dedup();
        }

        if query.is_empty() {
            error("No query specified");
        }

        // Toggle string search if the query contains no special characters
        // This is done because string search is faster than regex search
        if flags & 0b00001000 == 0 {
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

    let flags = cfg.flags;
    let case_insensitive = flags & 0b00000001 != 0;
    let show_lines = flags & 0b00000010 != 0;
    let invert = flags & 0b00000100 != 0;
    let string_search = flags & 0b00001000 != 0;

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
                    match_on,
                    i,
                    line,
                    "stdin",
                    query.as_bytes(),
                ) && max > 0
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
            error("No files specified");
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
                    match_on,
                    i,
                    line,
                    filename,
                    query.as_bytes(),
                ) && max > 0
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
            error(&format!("Error parsing regex: {}", err));
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
                    i,
                    line,
                    "stdin",
                    &re,
                ) && max > 0
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
            error("No files specified");
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
                    i,
                    line,
                    filename,
                    &re,
                ) && max > 0
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
            // write!(writer, "{}:{}:{}", filename, index + 1, line)
            write!(writer, "{}:{}", filename, index + 1)
        } else {
            write!(writer, "{}", filename)
        }
    } else if show_lines {
        write!(writer, "{}", index + 1)
    } else {
        Ok(())
    };
    writer.write_all(line).unwrap();
    if let Err(e) = res {
        error(&format!("Error writing to stdout: {}", e));
    }
}

fn read_file(filename: &str) -> LineReader<File> {
    let file = File::open(filename);
    if let Ok(file) = file {
        LineReader::new(file)
    } else {
        error(&format!("Error reading {}", filename));
        exit(1); // Required due to borrow checker
    }
}

fn error(message: &str) {
    eprintln!("{}", message);
    print_help();
    exit(1);
}

#[allow(clippy::too_many_arguments)]
fn check_string(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    case_insensitive: bool,
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
            if !line
                .split(|c| match c {
                    b' ' | b'\x09'..=b'\x0d' => true,
                    c => c > &b'\x7f',
                })
                .any(|word| (word == pattern) ^ invert)
            {
                return false;
            }
        }
    }
    print_match(writer, i, &line, show_lines, source, multiple_files);
    true
}

#[allow(clippy::too_many_arguments)]
fn check_regex(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    i: usize,
    line: &[u8],
    source: &str,
    pattern: &Regex,
) -> bool {
    if pattern.is_match(line) ^ invert {
        print_match(writer, i, line, show_lines, source, multiple_files);
        return true;
    }
    false
}
