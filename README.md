# rust\_grep

### Minimalist's grep written in Rust, inspired by ripgrep.

### Disclaimer

This project was made as a learning project.

As such, do not expect to find top quality code in this repo.

That being said, I do try my best to make the code as good as I know about.

This project is also unlikely to receive any updates as it is already "finished" with all the features I care about.

### Usage
```
Usage: rust_grep [OPTION]... QUERY [FILES]...
Search for PATTERN in FILES.
Example:
    rust_grep -i 'hello world' file1.txt file2.txt

Options:
-i          Ignore case distinctions in PATTERN
-n          Print line number with output lines
-v          Invert match: select non-matching lines
-F          String searching, disables regex
-x          Only match whole lines, only works with -F
-w          Only match whole words, only works with -F
-m=<NUM>    Stop after NUM matches
-h          Print this help and exit
```

### Features
+ Regex support
+ Reads from stdin so users can pipe programs' outputs into it (e.g. `seq 10000 | rust_grep '^\\d{1,3}$'`)
+ Customizable using command flags

### Todo(?)
+ Colour (Low Priority)
