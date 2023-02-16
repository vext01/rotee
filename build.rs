use std::env;

pub fn main() {
    if let Ok(profile) = env::var("PROFILE") {
        // Used in tests.
        println!("cargo:rustc-cfg=cargo_profile=\"{}\"", profile);
    }
}
