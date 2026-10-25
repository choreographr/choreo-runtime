fn main() {
    #[cfg(feature = "std")]
    {
        patch_frame_benchmarking_current_time();
        polkadot_sdk::substrate_wasm_builder::WasmBuilder::build_using_defaults();
    }
}

#[cfg(feature = "std")]
fn patch_frame_benchmarking_current_time() {
    use std::{env, fs, path::PathBuf};

    const OLD: &str = "\tfn current_time() -> AllocateAndReturnPointer<[u8; 16], 16> {\n\
\t\tstd::time::SystemTime::now()\n\
\t\t\t.duration_since(std::time::SystemTime::UNIX_EPOCH)\n\
\t\t\t.expect(\"Unix time doesn't go backwards; qed\")\n\
\t\t\t.as_nanos()\n\
\t\t\t.to_le_bytes()\n\
\t}\n";

    const NEW: &str = "\tfn current_time() -> AllocateAndReturnPointer<[u8; 16], 16> {\n\
\t\t#[cfg(feature = \"std\")]\n\
\t\t{\n\
\t\t\tstd::time::SystemTime::now()\n\
\t\t\t\t.duration_since(std::time::SystemTime::UNIX_EPOCH)\n\
\t\t\t\t.expect(\"Unix time doesn't go backwards; qed\")\n\
\t\t\t\t.as_nanos()\n\
\t\t\t\t.to_le_bytes()\n\
\t\t}\n\
\t\t#[cfg(not(feature = \"std\"))]\n\
\t\t{\n\
\t\t\t0u128.to_le_bytes()\n\
\t\t}\n\
\t}\n";

    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));

    let Some(cargo_home) = cargo_home else {
        return;
    };

    let mut candidates: std::vec::Vec<PathBuf> = std::vec::Vec::new();

    // Registry (crates.io) source — the current location of frame-benchmarking.
    // Perversely, the no_std `current_time()` bug (using `std::time::SystemTime`)
    // is still present in the published crate, so patch it here too.
    let registry_src = cargo_home.join("registry").join("src");
    if let Ok(index_dirs) = fs::read_dir(&registry_src) {
        for index_dir in index_dirs.flatten() {
            let index_path = index_dir.path();
            if !index_path.is_dir() {
                continue;
            }
            if let Ok(subdirs) = fs::read_dir(&index_path) {
                for sub in subdirs.flatten() {
                    let sub_path = sub.path();
                    if !sub_path.is_dir() {
                        continue;
                    }
                    let name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.starts_with("frame-benchmarking-") {
                        candidates.push(sub_path.join("src").join("utils.rs"));
                    }
                }
            }
        }
    }

    // Git checkouts (pre-crates.io layout) — kept as a fallback.
    let checkouts = cargo_home.join("git").join("checkouts");
    if checkouts.exists() {
        visit_dirs(&checkouts, &mut |dir| {
            let candidate = dir.join("substrate/frame/benchmarking/src/utils.rs");
            if candidate.exists() {
                candidates.push(candidate);
            }
        });
    }

    let mut patched_any = false;
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&candidate)
            && content.contains(OLD)
        {
            let updated = content.replacen(OLD, NEW, 1);
            if fs::write(&candidate, updated).is_ok() {
                patched_any = true;
            }
        }
    }

    if patched_any {
        println!("cargo:warning=Patched frame-benchmarking current_time for no_std wasm build");
    }
}

#[cfg(feature = "std")]
fn visit_dirs<F: FnMut(&std::path::Path)>(dir: &std::path::Path, cb: &mut F) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                cb(&path);
                visit_dirs(&path, cb);
            }
        }
    }
}
