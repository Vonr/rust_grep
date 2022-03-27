use std::{env::{self, Args}, process::exit};
use regex::{Regex, RegexBuilder};

struct Config {
    pub query: Option<Regex>,
    pub filenames: Vec<String>,
    pub show_lines: bool,
    pub max: u32,
    pub invert: bool,
    pub string_search: Option<String>,
}

impl Config {
    fn new(args: Args) -> Config {
        let mut expecting_max = false;
        let mut query = String::new();
        let mut filenames: Vec<String> = Vec::new();
        let mut case_sensitive = true;
        let mut show_lines = false;
        let mut max = 0;
        let mut invert = false;
        let mut is_string_search = false;

        for arg in args.skip(1) {
            if arg.starts_with("-") {
                let trimmed = arg.trim_start_matches("-");
                if trimmed.starts_with("m=") {
                    max = trimmed[2..].parse().unwrap_or(0);
                    continue;
                }
                for c in trimmed.chars() {
                    match c {
                        'm' => expecting_max = true,
                        'i' => case_sensitive = false,
                        'n' => show_lines = true,
                        'v' => invert = true,
                        'F' => is_string_search = true,
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
                if expecting_max {
                    max = arg.parse().unwrap_or(0);
                    expecting_max = false;
                } else if query.is_empty() {
                    query = arg;
                } else {
                    filenames.push(arg);
                }
            }
        }

        if query.is_empty() {
            error("No query specified");
        }

        if filenames.len() == 0 {
            error("No files specified");
        }

        if is_string_search {
            Config {
                query: None,
                filenames,
                show_lines,
                max,
                invert,
                string_search: Some(query),
            }
        } else {
            let re = RegexBuilder::new(&query)
                .case_insensitive(case_sensitive)
                .build();

            if re.is_err() {
                error(&format!("Invalid regex {}: {}", query, re.as_ref().err().unwrap()));
            }

            Config {
                query: Some(re.unwrap()),
                filenames,
                show_lines,
                max,
                invert,
                string_search: None,
            }
        }
    }
}

fn print_help() {
    println!("Usage: rust_grep [OPTION]... PATTERN [FILES]...\nSearch for PATTERN in FILES.\nExample:\n    rust_grep -i 'hello world' file1.txt file2.txt\n\nOptions:\n-i          Ignore case distinctions in PATTERN\n-n          Print line number with output lines\n-v          Invert match: select non-matching lines\n-F          String searching, disables regex\n-m <NUM>    Stop after NUM matches\n-h          Print this help and exit")
}

fn main() {
    let cfg = Config::new(env::args());

    print_matches(search(&cfg.query, cfg.filenames, cfg.show_lines, cfg.max, cfg.invert, cfg.string_search));
}

fn search(query: &Option<Regex>,
          filenames: Vec<String>,
          show_lines: bool,
          max: u32,
          invert: bool,
          string_search: Option<String>)
    -> Vec<String> {

        let multiple_files = filenames.len() > 1;
        let mut results = Vec::new();

        if let Some(string_search) = string_search {
            for filename in filenames {
                let mut matches: u32 = 0;
                let content = read_file(&filename);

                for (i, line) in content.lines().enumerate() {
                    if line.contains(&string_search) ^ invert {
                        results.push(format_line(i, &line, show_lines, &filename, multiple_files));
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                }

            }
        } else if let Some(query) = query {
            for filename in filenames {
                let mut matches: u32 = 0;
                let content = read_file(&filename);

                for (i, line) in content.lines().enumerate() {
                    if query.is_match(line) ^ invert {
                        results.push(format_line(i, &line, show_lines, &filename, multiple_files));
                        matches += 1;
                    }

                    if max > 0 && matches >= max {
                        break;
                    }
                }
            }
        } else {
            error("Invalid query provided");
        }

        results
    }

fn format_line(index: usize, line: &str, show_lines: bool, filename: &str, multiple_files: bool) -> String {
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
    let content = std::fs::read_to_string(filename);
    if content.is_err() {
        error(&format!("Error reading {}: {}", filename, content.err().unwrap()));
        return String::new();
    }

    content.unwrap()
}

fn print_matches(matches: Vec<String>) {
    for line in matches {
        println!("{}", line);
    }
}

fn error(message: &str) {
    eprintln!("{}", message);
    print_help();
    exit(1);
}
