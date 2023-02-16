use app::{App, Opt};
use libc::{sigemptyset, sigfillset, sigprocmask, sigset_t, SIG_BLOCK, SIG_SETMASK};
use std::{
    default::Default,
    error::Error,
    fs::{rename, File},
    io::{self, Read, Write},
    mem::MaybeUninit,
    path::PathBuf,
    process, ptr,
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

fn rotate(config: &Config, old_file: File, all_sigs: sigset_t) -> Result<File, Box<dyn Error>> {
    // `rotate_inner()` must not be interrupted, or output files may go missing. We block signals
    // that would kill us (the ones we can) until we are done rotating.
    //
    // We can use a full signal block set here. `sigprocmask` will ignore the unmaskable ones.
    let mut old_sigs = MaybeUninit::uninit();
    if unsafe { sigprocmask(SIG_BLOCK, &all_sigs, old_sigs.as_mut_ptr()) } == -1 {
        return Err("sigprocmask failed".into());
    }
    let old_sigs = unsafe { old_sigs.assume_init() };

    // Signals are now blocked. Do the rotation.
    let res = rotate_inner(config, old_file);

    // Restore the old signal mask.
    if unsafe { sigprocmask(SIG_SETMASK, &old_sigs, ptr::null_mut()) } == -1 {
        return Err("sigprocmask failed".into());
    }

    res.map_err(|e| e.into())
}

/// Rotate the output files, returning the freshly created file to use next.
fn rotate_inner(config: &Config, old_file: File) -> Result<File, io::Error> {
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

fn run(config: &Config) -> Result<(), Box<dyn Error>> {
    let mut of = File::create(outfile_path(&config.file_prefix, 0))?;
    let mut cur_size = 0;
    let mut buf = Vec::with_capacity(config.buffer_size);
    buf.resize(config.buffer_size, 0);

    // Compute the full set of signals for when we have to block signals.
    let mut all_sigs = MaybeUninit::uninit();
    if unsafe { sigemptyset(all_sigs.as_mut_ptr()) } == -1 {
        return Err("sigemptyset failed".into());
    }
    let mut all_sigs = unsafe { all_sigs.assume_init() };
    if unsafe { sigfillset(&mut all_sigs as *mut sigset_t) } == -1 {
        return Err("sigfillset failed".into());
    }

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
                        of = rotate(config, of, all_sigs)?;
                        cur_size = 0;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use libc::{kill, SIGTERM};
    use rand::Rng;
    use std::{env, fs::File, path::PathBuf, process::Command, thread, time::Duration};
    use tempfile::TempDir;

    #[cfg(cargo_profile = "release")]
    static CARGO_PROFILE: &str = "release";
    #[cfg(not(cargo_profile = "release"))]
    static CARGO_PROFILE: &str = "debug";

    /// Check (best we can) that delivering catchable signals cannot interrupt file rotation.
    /// https://github.com/vext01/rotee/issues/1
    #[test]
    fn test_signal() {
        let md = env::var("CARGO_MANIFEST_DIR").unwrap();
        let mut rng = rand::thread_rng();

        for _ in 0..50 {
            let p = [&md, "target", CARGO_PROFILE, "rotee"]
                .iter()
                .collect::<PathBuf>();
            let dir = TempDir::new().unwrap();
            env::set_current_dir(dir.path()).unwrap();
            let outfile0 = [dir.path().to_str().unwrap(), "rotee.0"]
                .iter()
                .collect::<PathBuf>();

            // Pipe /dev/zero into a rotee with a very small output file size, so that rotation
            // happens very frequently.
            let zero = File::open("/dev/zero").unwrap();
            let mut child = Command::new(p)
                .stdin(zero)
                .args(&["-s", "1", "-e"])
                .spawn()
                .unwrap();

            // Wait for `rotee.0` to appear for the first time.
            while !outfile0.exists() {
                thread::sleep(Duration::from_nanos(10));
            }

            // After a random amount of time, send rotee a catchable signal.
            thread::sleep(Duration::from_millis(rng.gen_range(0..101)));
            unsafe { kill(i32::try_from(child.id()).unwrap(), SIGTERM) };

            // rotee should exit with failure.
            assert!(!child.wait().unwrap().success());
            // and `rotee.0` should always exist.
            assert!(outfile0.exists());
        }
    }
}
