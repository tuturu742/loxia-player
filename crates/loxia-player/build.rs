//! Adds libmpv's `-L` search path (and a matching rpath) via `pkg-config` when it isn't on the
//! linker's default path (e.g. a Homebrew-on-Linux install) — see
//! `crates/loxia-audio/build.rs`'s identical (and more thoroughly commented) copy. `loxia`, as the
//! final binary linking libmpv transitively through `loxia-audio`, needs its own copy: a
//! dependency's `rustc-link-arg` (the rpath) applies only to that dependency's own compiled
//! artifacts, never to a downstream package's final binary.

fn main() {
    let _ = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("mpv")
        .map(|lib| {
            for path in lib.link_paths {
                println!("cargo:rustc-link-search=native={}", path.display());
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
            }
        });
}
