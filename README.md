# grepox
[![Crates.io](https://img.shields.io/crates/v/grepox)](https://crates.io/crates/grepox)

### Minimalist's grep written in Rust, inspired by ripgrep.

### Disclaimer

This project was made as a learning project.

That being said, I do try my best to make the code as good as I know about and may occasionally revisit with optimizations.

However, it is unlikely that large feature updates are made.

### Usage
```
Usage: grepox [OPTION]... QUERY [FILES]...
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
-h          Print this help and exit
```

### Features
+ Regex support
+ Reads from stdin so users can pipe programs' outputs into it (e.g. `seq 10000 | grepox '^\d{1,3}$'`)
+ Customizable using command flags
+ Colors
