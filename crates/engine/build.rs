//! Copies the built UI into the build directory so it can be embedded in
//! the program. When the UI has not been built, a placeholder page that
//! says so is embedded instead, so the engine always compiles.

use std::fs;
use std::io;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dist = Path::new(&manifest).join("../../ui/dist");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("ui");
    println!("cargo:rerun-if-changed={}", dist.display());

    let _ = fs::remove_dir_all(&out);
    if dist.join("index.html").exists() {
        copy_dir(&dist, &out).expect("could not copy the built UI");
    } else {
        fs::create_dir_all(&out).unwrap();
        fs::write(
            out.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>darkRGB</title>\
             <body style=\"font:16px system-ui;margin:3rem;max-width:40rem\">\
             <h1>The UI has not been built</h1>\
             <p>The engine is running, but this copy was compiled without its interface.</p>\
             <p>From the <code>ui</code> folder run <code>pnpm install</code> and <code>pnpm build</code>, \
             then build the engine again.</p>",
        )
        .unwrap();
        println!("cargo:warning=ui/dist not found; embedding a placeholder page. Build the UI first for a usable app.");
    }
}
