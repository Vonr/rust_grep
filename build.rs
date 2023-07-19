use std::path::Path;

use regex_automata::dfa::regex::Regex;

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let le_re_path = Path::new(&out_dir).join("plaintext_regex_le");
    let be_re_path = Path::new(&out_dir).join("plaintext_regex_be");

    let re = Regex::new(r#"[^[:alnum:] ;:~!@#%&\-_='",<>/]"#).unwrap();
    let dfa = re.forward();

    let (le_re, le_pad) = dfa.to_bytes_little_endian();
    let (be_re, be_pad) = dfa.to_bytes_big_endian();

    std::fs::write(le_re_path, &le_re[le_pad..]).unwrap();
    std::fs::write(be_re_path, &be_re[be_pad..]).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
