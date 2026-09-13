//! Adds libmpv's `-L` search path (and a matching rpath) via `pkg-config` when it isn't on the
//! linker's default path (e.g. a Homebrew-on-Linux install) — `libmpv2-sys`'s own build.rs emits
//! `-lmpv` unconditionally but never a search path or rpath, so the binary links but then fails
//! to *start* ("error while loading shared libraries") on such a system. A no-op, not a hard
//! requirement: on a system where libmpv already sits in the default path, this probe finding
//! nothing (or `pkg-config` being absent entirely) changes nothing, and both link and runtime
//! loading proceed exactly as they would have anyway.

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
