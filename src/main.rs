use crate::config::Config;
use crate::trait_ext::*;

use bstr::{io::BufReadExt, ByteSlice};
use regex::bytes::{Regex, RegexBuilder};
use std::{
    borrow::Cow,
    fs,
    io::{self, BufWriter, Read, StdoutLock, Write},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    process::{exit, ExitCode},
};

mod config;
mod trait_ext;

#[macro_export]
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
-U          No unicode, can speed up regular expressions
-q          Quiet, do not write to standard output.
            Exits immediately with 0 if any match is found
-m=<NUM>    Stop after NUM matches
-c          Colorizes output
-h          Print this help and exit"
        );
        ::std::process::exit(1)
    }};
}

#[macro_export]
macro_rules! error {
    ($format:literal$(, $args:expr)*) => {{
        eprintln!($format$(, $args)*);
        $crate::print_help!();
    }};
}

#[derive(PartialEq, Clone, Copy)]
enum MatchOn {
    Anywhere,
    Line,
    Word,
}

fn grep(cfg: Config) -> ExitCode {
    let multiple_files = cfg.flag(7) || cfg.filenames.len() > 1;

    let case_insensitive = cfg.flag(0);
    let show_lines = cfg.flag(1);
    let invert = cfg.flag(2);
    let string_search = cfg.flag(3);
    let color = cfg.flag(4);
    let no_unicode = !cfg.flag(5);
    let quiet = cfg.flag(6);

    let mut total_matches: u32 = 0;
    let query = cfg.query;
    let max = cfg.max;
    let has_max = max > 0;
    let filenames = cfg.filenames;
    let match_on = cfg.match_on;

    let is_tty = atty::is(atty::Stream::Stdin);

    let stdout = std::io::stdout();
    let stdout = stdout.lock();
    let mut writer = BufWriter::with_capacity(16384, stdout);
    let mut buf = Vec::new();

    if string_search {
        let query = if case_insensitive {
            query.to_lowercase()
        } else {
            query
        };
        let query = &query.into_bytes();
        if !is_tty {
            let mut i = 0;
            let filename = Path::new("stdin");
            let _ = {
                let stdin = io::stdin();
                let mut stdin = stdin.lock();
                stdin.for_byte_line_with_terminator(|line| {
                    if has_max && total_matches >= max {
                        return Ok(false);
                    }

                    if check_string(
                        &mut buf,
                        &mut writer,
                        quiet,
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
                    ) {
                        total_matches += 1;
                    }

                    i += 1;
                    Ok(true)
                })
            };
            return ExitCode::from_bool(total_matches > 0);
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        let mut reader = Vec::new();
        for filename in &filenames {
            let mut matches: u32 = 0;
            read_file(&mut reader, filename);
            let mut i = 0;

            let _ = reader.as_slice().for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_string(
                    &mut buf,
                    &mut writer,
                    quiet,
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
                ) {
                    matches += 1;
                    total_matches += 1;
                }

                i += 1;
                Ok(true)
            });
        }
    } else {
        let re = RegexBuilder::new(&query)
            .unicode(no_unicode)
            .case_insensitive(case_insensitive)
            .multi_line(true)
            .build();

        let re = &match re {
            Err(err) => error!("Error parsing regex: {}", err),
            Ok(re) => re,
        };

        if !is_tty {
            let mut i = 0;
            let filename = Path::new("stdin");
            let _ = {
                let stdin = io::stdin();
                let mut stdin = stdin.lock();
                stdin.for_byte_line_with_terminator(|line| {
                    if has_max && total_matches >= max {
                        return Ok(false);
                    }

                    if check_regex(
                        &mut buf,
                        &mut writer,
                        quiet,
                        show_lines,
                        multiple_files,
                        invert,
                        color,
                        i,
                        line,
                        filename,
                        re,
                    ) {
                        total_matches += 1;
                    }

                    i += 1;
                    Ok(true)
                })
            };
            return ExitCode::from_bool(total_matches > 0);
        }

        if filenames.is_empty() {
            error!("No files specified");
        }

        let mut reader = Vec::new();
        for filename in &filenames {
            let mut matches: u32 = 0;
            read_file(&mut reader, filename);
            let mut i = 0;

            let _ = reader.as_slice().for_byte_line_with_terminator(|line| {
                if has_max && matches >= max {
                    return Ok(false);
                }

                if check_regex(
                    &mut buf,
                    &mut writer,
                    quiet,
                    show_lines,
                    multiple_files,
                    invert,
                    color,
                    i,
                    line,
                    filename,
                    re,
                ) {
                    matches += 1;
                    total_matches += 1;
                }

                i += 1;
                Ok(true)
            });
        }
    }
    writer.flush().unwrap();
    ExitCode::from_bool(total_matches > 0)
}

fn print_match(
    writer: &mut BufWriter<StdoutLock>,
    index: usize,
    line: &[u8],
    show_lines: bool,
    filename: &Path,
    multiple_files: bool,
) {
    let res = if multiple_files {
        writer
            .write_all(filename.as_os_str().as_bytes())
            .and_then(|_| {
                if show_lines {
                    write!(writer, ":{}:", index + 1)
                } else {
                    writer.write_all(b":")
                }
            })
    } else if show_lines {
        write!(writer, "{}:", index + 1)
    } else {
        Ok(())
    };
    if let Err(e) = res.and_then(|_| writer.write_all(line)) {
        error!("Error writing to stdout: {}", e);
    }
}

fn read_file(buf: &mut Vec<u8>, filename: &PathBuf) {
    let mut file = fs::File::open(filename).unwrap_or_else(|e| error!("Error reading file: {}", e));

    let needed = file.metadata().map(|m| m.len()).unwrap_or(0);
    let needed: usize = needed
        .try_into()
        .unwrap_or_else(|_| error!("File too big: {}", needed));

    if buf.reserve_total(needed).is_err() {
        error!("Could not allocate {needed} bytes");
    }
    buf.clear();

    if let Err(e) = file.read_to_end(buf) {
        error!("Error reading file: {}", e);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_string(
    buf: &mut Vec<u8>,
    writer: &mut BufWriter<StdoutLock>,
    quiet: bool,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    case_insensitive: bool,
    color: bool,
    match_on: MatchOn,
    i: usize,
    line: &[u8],
    source: &Path,
    pattern: &[u8],
) -> bool {
    let line = if case_insensitive {
        Cow::Owned(line.to_ascii_lowercase())
    } else {
        Cow::Borrowed(line)
    };

    match (match_on, !color || invert) {
        (_, true) | (MatchOn::Line, _) => {
            match match_on {
                MatchOn::Anywhere => {
                    if !(&*line).contains_str(pattern) ^ invert {
                        return false;
                    }
                }
                MatchOn::Line => {
                    if (line != pattern) ^ invert {
                        return false;
                    }
                }
                MatchOn::Word => {
                    if line.words().all(|word| (word != pattern) ^ invert) {
                        return false;
                    }
                }
            }

            if quiet {
                exit(0);
            }
            print_match(writer, i, &line, show_lines, source, multiple_files);
            return true;
        }
        (MatchOn::Anywhere, _) => {
            let line = &*line;
            let indices = line.find_iter(pattern).collect::<Vec<_>>();
            if indices.is_empty() {
                return false;
            } else if quiet {
                exit(0);
            }

            let needed = line.len() + indices.len() * 10;
            if buf.reserve_total(needed).is_err() {
                error!("Could not allocate {needed} bytes");
            }
            buf.clear();
            let mut last = 0;
            let len = pattern.len();

            unsafe {
                for idx in indices.into_iter() {
                    buf.extend_from_slice_unchecked(&line[last..idx]);
                    buf.extend_from_slice_unchecked(b"\x1b[31;1m");
                    buf.extend_from_slice_unchecked(pattern);
                    buf.extend_from_slice_unchecked(b"\x1b[m");
                    last = idx + len;
                }
                buf.extend_from_slice_unchecked(&line[last..]);
            }

            print_match(writer, i, buf, show_lines, source, multiple_files);
        }
        (MatchOn::Word, _) => {
            buf.clear();
            let mut found = false;
            for word in line.words() {
                if word == pattern {
                    if quiet {
                        exit(0);
                    }
                    found = true;
                    let _ = buf
                        .write_all(b"\x1b[31;1m")
                        .and_then(|_| buf.write_all(word))
                        .and_then(|_| buf.write_all(b"\x1b[m "));
                } else {
                    let _ = buf.write_all(word).map(|_| buf.push(b' '));
                }
            }
            if !found {
                return false;
            }
            buf.push(b'\n');
            print_match(writer, i, buf, show_lines, source, multiple_files);
        }
    };
    true
}

#[allow(clippy::too_many_arguments)]
fn check_regex(
    buf: &mut Vec<u8>,
    writer: &mut BufWriter<StdoutLock>,
    quiet: bool,
    show_lines: bool,
    multiple_files: bool,
    invert: bool,
    color: bool,
    i: usize,
    line: &[u8],
    source: &Path,
    pattern: &Regex,
) -> bool {
    if quiet && pattern.is_match(line) ^ invert {
        exit(0);
    }
    if color && !invert {
        let indices: Vec<(usize, usize)> = pattern
            .find_iter(line)
            .map(|loc| (loc.start(), loc.end()))
            .collect();
        if indices.is_empty() {
            return false;
        }

        let colored = buf;
        let needed = line.len() + indices.len() * 10;
        if colored.reserve_total(needed).is_err() {
            error!("Could not allocate {needed} bytes");
        }
        colored.clear();

        let mut last = 0;
        unsafe {
            for (start, end) in indices.into_iter() {
                colored.extend_from_slice_unchecked(&line[last..start]);
                colored.extend_from_slice_unchecked(b"\x1b[31;1m");
                colored.extend_from_slice_unchecked(&line[start..end]);
                colored.extend_from_slice_unchecked(b"\x1b[m");
                last = end
            }
            colored.extend_from_slice_unchecked(&line[last..]);
        }

        print_match(writer, i, colored, show_lines, source, multiple_files);
        return true;
    }
    if pattern.is_match(line) ^ invert {
        print_match(writer, i, line, show_lines, source, multiple_files);
        return true;
    }
    false
}

fn main() -> ExitCode {
    let config = Config::new();
    grep(config)
}
