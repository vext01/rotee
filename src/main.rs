use app::{App, Opt};
use std::{
    default::Default,
    fs::{rename, File},
    io::{self, Read, Write},
    path::PathBuf,
    process,
};

struct Config {
    /// The prefix of output filenames.
    file_prefix: String,
    /// The size (in bytes) at which files will be rotated.
    file_size: usize,
    /// The maximum number of files to use in rotation.
    num_files: usize,
    /// Do not echo input back to stdout.
    no_echo: bool,
    /// Buffer size used for reading from stdin.
    buffer_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            file_prefix: String::from("rotee."),
            file_size: 1024 * 1024 * 8, // 8 MiB
            num_files: 8,
            no_echo: false,
            buffer_size: 1024 * 1024, // 1 MiB
        }
    }
}

fn fatal(m: &str) {
    eprintln!("error: {}", m);
    process::exit(1);
}

fn outfile_path(prefix: &str, suffix: usize) -> PathBuf {
    PathBuf::from(format!("{}{}", prefix, suffix))
}

fn rotate(config: &Config, old_file: File) -> Result<File, io::Error> {
    drop(old_file);

    for i in (0..(config.num_files - 1)).rev() {
        let old_path = outfile_path(&config.file_prefix, i);
        if old_path.exists() {
            let new_path = outfile_path(&config.file_prefix, i + 1);
            rename(old_path, new_path)?;
        }
    }
    Ok(File::create(outfile_path(&config.file_prefix, 0))?)
}

fn main() {
    let mut config = Config::default();

    App::new("rotee")
        .desc("Split stdin between rotating output files")
        .opt(
            Opt::new("buf-size", &mut config.buffer_size)
                .short('b')
                .help("size of the buffer used to read from stdin"),
        )
        .opt(
            Opt::new("no-echo", &mut config.no_echo)
                .short('e')
                .help("do not re-echo stdout"),
        )
        .opt(
            Opt::new("num-files", &mut config.num_files)
                .short('n')
                .help("maximum number of files to use"),
        )
        .opt(
            Opt::new("file-prefix", &mut config.file_prefix)
                .short('p')
                .help("output filename prefix"),
        )
        .opt(
            Opt::new("file-size", &mut config.file_size)
                .short('s')
                .help("size (in bytes) after which to rotate output files"),
        )
        .parse_args();

    if config.buffer_size == 0 {
        fatal("buffer size (-b) must be non-zero");
    }

    if config.num_files == 0 {
        fatal("number of files (-n) must be non-zero");
    }

    if config.file_size == 0 {
        fatal("file size (-s) must be non-zero");
    }

    if let Err(e) = run(&config) {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}

fn run(config: &Config) -> Result<(), io::Error> {
    let mut of = File::create(outfile_path(&config.file_prefix, 0))?;
    let mut cur_size = 0;
    let mut buf = Vec::with_capacity(config.buffer_size);
    buf.resize(config.buffer_size, 0);

    loop {
        match io::stdin().read(&mut buf)? {
            0 => break, // EOF.
            nbytes => {
                let mut idx = 0;
                while idx < nbytes {
                    let write_size = usize::min(nbytes - idx, config.file_size - cur_size);
                    let bytes = &buf[idx..(idx + write_size)];
                    of.write_all(bytes)?;
                    if !config.no_echo {
                        io::stdout().write_all(&bytes)?;
                    }

                    idx += write_size;
                    cur_size += write_size;
                    if cur_size >= config.file_size {
                        of = rotate(config, of)?;
                        cur_size = 0;
                    }
                }
            }
        }
    }
    Ok(())
}
