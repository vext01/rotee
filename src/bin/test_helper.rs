//! Test helper that runs rotee and collects the output files (including stdout and stderr) onto
//! stdout so that we can use lang_tester to match the output.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{exit, Command, Output},
};
use tempfile;

fn bin() -> PathBuf {
    let md = env::var("CARGO_MANIFEST_DIR").unwrap();
    let kind = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    [&md, "target", kind, "rotee"].iter().collect::<PathBuf>()
}

fn run_rotee(infile: &str) -> Output {
    let infile = PathBuf::from(infile);
    if !infile.is_absolute() {
        panic!("path to test input must be absolute");
    }

    let mut cmd = Command::new(bin());
    cmd.stdin(fs::File::open(infile).unwrap());

    if let Ok(args) = env::var("ROTEE_ARGS") {
        for arg in args.split(" ") {
            cmd.arg(arg);
        }
    }

    cmd.output().unwrap()
}

fn emit(dir: &Path, output: &Output) {
    let mut paths = fs::read_dir(dir)
        .unwrap()
        .map(|r| r.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    if !stdout.is_empty() {
        println!(">>> stdout");
        print!("{}", stdout);
        if !stdout.ends_with('\n') {
            println!("<no-eol>");
        }
    }

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    if !stderr.is_empty() {
        println!(">>> stderr");
        print!("{}", stderr);
        if !stderr.ends_with('\n') {
            println!("<no-eol>");
        }
    }

    for path in paths {
        println!(">>> {}", path.file_name().unwrap().to_str().unwrap());
        let fc = fs::read_to_string(&path).unwrap();
        print!("{}", fc);
        if !fc.ends_with('\n') {
            println!("<no-eol>");
        }
    }
}

fn main() {
    let tempdir = tempfile::tempdir().unwrap();
    env::set_current_dir(tempdir.path()).unwrap();
    let output = run_rotee(&env::args().skip(1).next().unwrap());
    emit(tempdir.path(), &output);
    exit(output.status.code().unwrap_or(1));
}
