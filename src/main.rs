use std::env::{self, Args};
use regex::{Regex, RegexBuilder};

struct Config {
    pub query: Regex,
    pub filename: String,
    pub show_lines: bool,
    pub max: u32,
}

impl Config {
    fn new(args: Args) -> Config {
        let mut expecting_max = false;
        let mut query = String::new();
        let mut filename = String::new();
        let mut case_sensitive = true;
        let mut show_lines = false;
        let mut max = 0;

        for arg in args.skip(1) {
            if arg.starts_with("-") {
                let trimmed = arg.trim_start_matches("-");
                match trimmed {
                    "m" => expecting_max = true,
                    _ => {
                        for c in trimmed.chars() {
                            match c {
                                'i' => case_sensitive = false,
                                'n' => show_lines = true,
                                _ => panic!("Unrecognized flag: {}", c),
                            }
                        }
                    }
                }
            } else {
                if expecting_max {
                    max = arg.parse::<u32>().unwrap();
                    expecting_max = false;
                } else if query.is_empty() {
                    query = arg;
                } else if filename.is_empty() {
                    filename = arg;
                } else {
                    panic!("Too many arguments");
                }
            }
        }

        let re = RegexBuilder::new(&query)
            .case_insensitive(case_sensitive)
            .build()
            .expect("Invalid regex query provided");
        Config {
            query: re,
            filename,
            show_lines,
            max,
        }
    }
}

fn main() {
    let cfg = Config::new(env::args());

    print_matches(search(cfg.query, &cfg.filename, cfg.show_lines, cfg.max));
}

fn search(query: Regex, filename: &str, show_lines: bool, max: u32) -> Vec<String> {
    let content = std::fs::read_to_string(filename).expect("Something went wrong reading the file");

    let mut results = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if query.is_match(line) {
            results.push(
                if show_lines {
                    format!("{}:{}", i+1, line)
                } else {
                    line.to_string()
                }
            );
        }

        if max > 0 && results.len() >= max as usize {
            break;
        }
    }

    results
}

fn print_matches(matches: Vec<String>) {
    for line in matches {
        println!("{}", line);
    }
}
