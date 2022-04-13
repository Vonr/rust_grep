use naive_opt::Search;
use regex::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    env::{self, Args},
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, StdoutLock, Write},
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
                    for c in trimmed.chars() {
                        match c {
                            'i' => flags |= 0b00000001,
                            'n' => flags |= 0b00000010,
                            'v' => flags |= 0b00000100,
                            'F' => flags |= 0b00001000,
                            'w' => match_on = MatchOn::Word,
                            'x' => match_on = MatchOn::Line,
                            'h' => {
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
    let case_insensitive = flags & 0b00000001 > 0;
    let show_lines = flags & 0b00000010 > 0;
    let invert = flags & 0b00000100 > 0;
    let string_search = flags & 0b00001000 > 0;

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
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            let mut line = String::new();
            let mut i = 0;
            while let Ok(bytes) = stdin.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }

                let cleaned = clean_string(&line);
                if check_string(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    case_insensitive,
                    match_on,
                    i,
                    cleaned,
                    "stdin",
                    &query,
                ) && max > 0
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                line.clear();
                i += 1;
            }
            return;
        }

        if filenames.is_empty() {
            error("No files specified");
        }

        let mut line = String::new();
        for filename in &filenames {
            let mut matches: u32 = 0;
            let reader = &mut read_file(filename);

            let mut i = 0;
            while let Ok(bytes) = reader.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }

                let cleaned = clean_string(&line);
                if check_string(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    case_insensitive,
                    match_on,
                    i,
                    cleaned,
                    filename,
                    &query,
                ) && max > 0
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                line.clear();
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
            let stdin = io::stdin();
            let mut stdin = stdin.lock();
            let mut line = String::new();
            let mut i = 0;
            while let Ok(bytes) = stdin.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }

                let cleaned = clean_string(&line);
                if check_regex(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    i,
                    cleaned,
                    "stdin",
                    &re,
                ) && max > 0
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                line.clear();
                i += 1;
            }
            return;
        }

        if filenames.is_empty() {
            error("No files specified");
        }

        let mut line = String::new();
        for filename in &filenames {
            let mut matches: u32 = 0;
            let reader = &mut read_file(filename);

            let mut i = 0;
            while let Ok(bytes) = reader.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }
                let cleaned = clean_string(&line);
                if check_regex(
                    &mut writer,
                    show_lines,
                    multiple_files,
                    invert,
                    i,
                    cleaned,
                    filename,
                    &re,
                ) && max > 0
                {
                    matches += 1;
                    if matches >= max {
                        break;
                    }
                }

                line.clear();
                i += 1;
            }
        }
    }
    writer.flush().unwrap();
}

fn print_match(
    writer: &mut BufWriter<StdoutLock>,
    index: usize,
    line: &str,
    show_lines: bool,
    filename: &str,
    multiple_files: bool,
) {
    let res = if multiple_files {
        if show_lines {
            writeln!(writer, "{}:{}:{}", filename, index + 1, line)
        } else {
            writeln!(writer, "{}:{}", filename, line)
        }
    } else if show_lines {
        writeln!(writer, "{}:{}", index + 1, line)
    } else {
        writeln!(writer, "{}", line)
    };
    if let Err(e) = res {
        error(&format!("Error writing to stdout: {}", e));
    }
}

fn read_file(filename: &str) -> BufReader<File> {
    let file = File::open(filename);
    if let Ok(file) = file {
        BufReader::new(file)
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

fn clean_string(s: &str) -> &str {
    s.trim_end_matches(|c| c == '\n' || c == '\r')
}

fn check_string(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    case_insensitive: bool,
    match_on: MatchOn,
    i: usize,
    line: &str,
    source: &str,
    pattern: &str,
) -> bool {
    let line = if case_insensitive {
        Cow::Owned(line.to_lowercase())
    } else {
        line.into()
    };
    if (match_on == MatchOn::Anywhere && (&*line).includes(pattern) ^ invert)
        || (match_on == MatchOn::Line && (line == *pattern) ^ invert)
        || (match_on == MatchOn::Word
            && line
                .split_whitespace()
                .any(|word| (word == pattern) ^ invert))
    {
        print_match(writer, i, &line, show_lines, source, multiple_files);
        return true;
    }
    false
}

fn check_regex(
    writer: &mut BufWriter<StdoutLock>,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    i: usize,
    line: &str,
    source: &str,
    pattern: &Regex,
) -> bool {
    if pattern.is_match(line) ^ invert {
        print_match(writer, i, line, show_lines, source, multiple_files);
        return true;
    }
    false
}
