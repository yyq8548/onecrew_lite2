//! Embed the built browser frontend so a plugin needs only one native binary.
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn collect(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(std::io::Error::other(
                "web assets must not contain symlinks",
            ));
        }
        if kind.is_dir() {
            collect(root, &entry.path(), files)?;
        } else if kind.is_file() {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
        .join("../../apps/web/dist");
    if !root.join("index.html").is_file() {
        panic!(
            "Build the OneCrew browser assets first: npm --prefix apps/web ci && npm --prefix apps/web run build"
        );
    }
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(&root, &root, &mut files).expect("read web build assets");
    files.sort();
    let mut source = String::from("pub(super) static WEB_ASSETS: &[(&str, &[u8])] = &[\n");
    for file in files {
        let key = file.to_string_lossy().replace('\\', "/");
        let path = root.join(&file).canonicalize().expect("web asset path");
        source.push_str(&format!(
            "({key:?}, include_bytes!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    source.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").expect("Cargo output directory")).join("web_assets.rs"),
        source,
    )
    .expect("write web asset manifest");
}
