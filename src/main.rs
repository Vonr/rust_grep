use regex::{Regex, RegexBuilder};
use std::{
    env::{self, Args},
    io::{self, BufRead},
    process::exit,
};

struct Config {
    pub query: String,
    pub filenames: Vec<String>,
    pub show_lines: bool,
    pub max: u32,
    pub invert: bool,
    pub case_sensitive: bool,
    pub is_string_search: bool,
    pub is_pattern_file: bool,
    pub match_on: MatchOn,
}

#[derive(PartialEq)]
enum MatchOn {
    Anywhere,
    Line,
    Word,
}

impl Config {
    fn new(args: Args) -> Config {
        let mut query = String::new();
        let mut filenames: Vec<String> = Vec::new();
        let mut show_lines = false;
        let mut max = 0;
        let mut invert = false;
        let mut case_sensitive = true;
        let mut is_string_search = false;
        let mut is_pattern_file = false;
        let mut match_on = MatchOn::Anywhere;

        for arg in args.skip(1) {
            if arg.starts_with("-") {
                let trimmed = arg.trim_start_matches("-");
                if trimmed.starts_with("m=") {
                    max = trimmed[2..].parse().unwrap_or(0);
                    continue;
                }
                for c in trimmed.chars() {
                    match c {
                        'i' => case_sensitive = false,
                        'n' => show_lines = true,
                        'v' => invert = true,
                        'F' => is_string_search = true,
                        'f' => is_pattern_file = true,
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
            } else {
                if query.is_empty() {
                    query = arg;
                } else {
                    filenames.push(arg);
                }
            }
        }

        if query.is_empty() {
            error("No query specified");
        }

        Config {
            query,
            filenames,
            show_lines,
            max,
            invert,
            case_sensitive,
            is_string_search,
            is_pattern_file,
            match_on,
        }
    }
}

fn print_help() {
    println!(concat!(
        "Usage: rust_grep [OPTION]... QUERY [FILES]...\n",
        "Search for PATTERN in FILES.\n",
        "Example:\n",
        "    rust_grep -i 'hello world' file1.txt file2.txt\n\n",
        "Options:\n",
        "-i          Ignore case distinctions in PATTERN\n",
        "-n          Print line number with output lines\n",
        "-v          Invert match: select non-matching lines\n",
        "-F          String searching, disables regex\n",
        "-f          Read patterns from file specified in QUERY\n",
        "-x          Only match whole lines, only works with -F\n",
        "-w          Only match whole words, only works with -F\n",
        "-m <NUM>    Stop after NUM matches\n",
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
    let invert = cfg.invert;
    let show_lines = cfg.show_lines;
    let max = cfg.max;
    let match_on = cfg.match_on;

    let istty = atty::is(atty::Stream::Stdin);

    if cfg.is_string_search {
        let mut patterns = Vec::new();
        if cfg.is_pattern_file {
            patterns.append(&mut read_patterns_file_string(&query));
        } else {
            patterns.push(query);
        }

        if !istty {
            let mut stdin = io::stdin().lock();
            for pattern in &patterns {
                let mut line = String::new();
                let mut i: usize = 0;
                while let Ok(bytes_read) = stdin.read_line(&mut line) {
                    if bytes_read == 0 {
                        break;
                    }
                    line = line.trim_end().to_string();
                    if (match_on == MatchOn::Anywhere && line.contains(pattern) ^ invert)
                        || (match_on == MatchOn::Line && (line == pattern.to_string()) ^ invert)
                        || (match_on == MatchOn::Word
                            && line
                                .split_whitespace()
                                .any(|word| (word == pattern) ^ invert))
                    {
                        println!(
                            "{}",
                            format_line(i, &line, cfg.show_lines, "stdin", multiple_files)
                        );
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                    i += 1;
                    line.clear();
                }
            }
            return;
        }

        if filenames.len() == 0 {
            error("No files specified");
        }

        for pattern in patterns {
            for filename in &filenames {
                let mut printed = Vec::new();
                let mut matches: u32 = 0;
                let content = read_file(&filename);

                for (i, line) in content.lines().enumerate() {
                    if !printed.contains(&i)
                        && (match_on == MatchOn::Anywhere && line.contains(&pattern) ^ cfg.invert)
                        || (match_on == MatchOn::Line && (line == pattern) ^ cfg.invert)
                        || (match_on == MatchOn::Word
                            && line
                                .split_whitespace()
                                .any(|word| (word == &pattern) ^ cfg.invert))
                    {
                        println!(
                            "{}",
                            format_line(i, line, cfg.show_lines, "stdin", multiple_files)
                        );
                        printed.push(i);
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                }
            }
        }
    } else {
        let mut patterns = Vec::new();
        if cfg.is_pattern_file {
            patterns.append(&mut read_patterns_file_regex(&query, cfg.case_sensitive));
        } else {
            let re = RegexBuilder::new(&query)
                .case_insensitive(!cfg.case_sensitive)
                .build();

            if re.is_err() {
                error(&format!("Error parsing regex: {}", re.err().unwrap()));
                return;
            }
            patterns.push(re.unwrap());
        }

        if !istty {
            let mut stdin = io::stdin().lock();
            for pattern in &patterns {
                let mut line = String::new();
                let mut i: usize = 0;
                while let Ok(bytes_read) = stdin.read_line(&mut line) {
                    if bytes_read == 0 {
                        break;
                    }
                    line = line.trim_end().to_string();
                    if pattern.is_match(&line) ^ invert {
                        println!(
                            "{}",
                            format_line(i, &line, cfg.show_lines, "stdin", multiple_files)
                        );
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                    i += 1;
                    line.clear();
                }
            }
            return;
        }

        if filenames.len() == 0 {
            error("No files specified");
        }

        for pattern in patterns {
            for filename in &filenames {
                let mut printed = Vec::new();
                let mut matches: u32 = 0;
                let content = read_file(&filename);

                for (i, line) in content.lines().enumerate() {
                    if !printed.contains(&i) && pattern.is_match(line) ^ invert {
                        println!(
                            "{}",
                            format_line(i, line, show_lines, &filename, multiple_files)
                        );
                        printed.push(i);
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                }
            }
        }
    }
}

fn format_line(
    index: usize,
    line: &str,
    show_lines: bool,
    filename: &str,
    multiple_files: bool,
) -> String {
    if multiple_files {
        if show_lines {
            format!("{}:{}:{}", filename, index + 1, line)
        } else {
            format!("{}:{}", filename, line)
        }
    } else {
        if show_lines {
            format!("{}:{}", index + 1, line)
        } else {
            format!("{}", line)
        }
    }
}

fn read_file(filename: &str) -> String {
    let contents = std::fs::read_to_string(filename);
    if contents.is_err() {
        error(&format!(
            "Error reading {}: {}",
            filename,
            contents.err().unwrap()
        ));
        exit(1) // Required because of borrow checker
    }

    contents.unwrap()
}

fn read_patterns_file_regex(filename: &str, case_sensitive: bool) -> Vec<Regex> {
    let content = std::fs::read_to_string(filename);
    if content.is_err() {
        error(&format!(
            "Error reading {}: {}",
            filename,
            content.err().unwrap()
        ));
        return Vec::new();
    }

    let mut patterns = Vec::new();
    for (i, line) in content.unwrap().lines().enumerate() {
        let re = RegexBuilder::new(line)
            .case_insensitive(!case_sensitive)
            .build();

        if re.is_err() {
            error(&format!(
                "Error parsing regex: {} in {}:{}",
                re.err().unwrap(),
                &filename,
                i
            ));
            return Vec::new();
        }

        patterns.push(re.unwrap())
    }

    patterns
}

fn read_patterns_file_string(filename: &str) -> Vec<String> {
    let content = std::fs::read_to_string(filename);
    if content.is_err() {
        error(&format!(
            "Error reading {}: {}",
            filename,
            content.err().unwrap()
        ));
        return Vec::new();
    }

    content
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect()
}

fn error(message: &str) {
    eprintln!("{}", message);
    print_help();
    exit(1);
}
