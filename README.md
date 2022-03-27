# rust\_grep

### Minimalist's grep written in Rust, inspired by ripgrep.

### Disclaimer:

This project was made as a learning project.

As such, do not expect to find top quality code in this repo.

That being said, I do try my best to make the code as good as I know about.

This project is also unlikely to receive any updates as it is already "finished" with all the features I care about.

### Usage
```
Usage: rust_grep [OPTION]... PATTERN [FILES]...
Search for PATTERN in FILES.
Example:
    rust_grep -i 'hello world' file.txt

Options:
-i          Ignore case distinctions in PATTERN
-n          Print line number with output lines
-v          Invert match: select non-matching lines
-m <NUM>    Stop after NUM matches
-h          Print this help and exit```
```

### Features
+ Regex support
+ Customizable using command flags
    + Ignore casing with the -i flag
    + Show the line in which the match is found with the -n flag
    + Specify the maximum number of matches to be found with the -m <max\_num> flag
    + String searching with -F flag (disables regex search)
    + Inversion of pattern with -v flag

### Todo(?)
+ Colour (Low Priority)
