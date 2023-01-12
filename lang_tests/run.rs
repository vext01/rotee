use lang_tester::LangTester;
use std::{env, fs::read_to_string, path::PathBuf, process::Command};

fn helper_path() -> PathBuf {
    let md = env::var("CARGO_MANIFEST_DIR").unwrap();
    let kind = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    [&md, "target", kind, "test_helper"]
        .iter()
        .collect::<PathBuf>()
}

fn run(block_size: &'static str) {
    // We use rustc to compile files into a binary: we store those binary files
    // into `tempdir`. This may not be necessary for other languages.
    LangTester::new()
        .test_dir("lang_tests/tests")
        .test_file_filter(|p| p.extension().unwrap().to_str().unwrap() == "in")
        .test_extract(|p| {
            let mut p = p.to_owned();
            p.set_extension("expect");
            if !p.exists() {
                panic!(
                    "expected output file doesn't exist: {}",
                    p.to_str().unwrap()
                );
            }
            read_to_string(p)
                .unwrap()
                .lines()
                .collect::<Vec<_>>()
                .join("\n")
        })
        .test_cmds(move |p| {
            let mut helper = Command::new(helper_path());
            helper.arg(p.to_str().unwrap());
            helper.env("ROTEE_BLOCKSIZE", block_size);
            vec![("Helper", helper)]
        })
        .run();
}

fn main() {
    for bs in ["1", "10", "100", "1024", "1048576", "8388608"] {
        println!("Running tests with block size {}", bs);
        run(bs);
    }
}
