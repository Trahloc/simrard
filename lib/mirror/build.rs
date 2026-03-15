// build.rs for simrard-lib-mirror
//
// Policy enforcement: DuckDB must be installed as a SYSTEM library.
// The `bundled` feature is NEVER acceptable — it recompiles all of DuckDB
// from C++ source on every clean build, spiking every core on the machine.
//
// This build script gates the crate on system libduckdb being present so that:
//   1. Phase 4.D2 can safely add `duckdb = "x.y"` (no `bundled`) knowing the
//      library is already verified on this machine.
//   2. Anyone who accidentally adds `bundled` back will see this file and
//      understand why it is forbidden.
//
// On omarchy (Arch-based), install the system library with:
//   sudo pacman -S duckdb
//
// Then rebuild:
//   cargo build

fn main() {
    // Preferred path: pkg-config metadata exists and we use it.
    if pkg_config::probe_library("duckdb").is_ok() {
        return;
    }

    // Arch/omarchy duckdb package may provide libduckdb.so without duckdb.pc.
    let common_lib_paths = ["/usr/lib", "/usr/lib64", "/lib", "/lib64"];
    if common_lib_paths
        .iter()
        .any(|path| std::path::Path::new(path).join("libduckdb.so").exists())
    {
        // Tell Cargo/rustc to link against system libduckdb directly.
        println!("cargo:rustc-link-lib=dylib=duckdb");
        for path in common_lib_paths {
            if std::path::Path::new(path).join("libduckdb.so").exists() {
                println!("cargo:rustc-link-search=native={path}");
            }
        }
        return;
    }

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!("║           MISSING SYSTEM DEPENDENCY: DuckDB             ║");
    eprintln!("╠══════════════════════════════════════════════════════════╣");
    eprintln!("║  Could not find system libduckdb.                       ║");
    eprintln!("║                                                          ║");
    eprintln!("║  On omarchy (Arch-based), install it with:              ║");
    eprintln!("║    sudo pacman -S duckdb                                 ║");
    eprintln!("║                                                          ║");
    eprintln!("║  If already installed, ensure libduckdb.so is in:       ║");
    eprintln!("║    /usr/lib or /usr/lib64                                ║");
    eprintln!("║                                                          ║");
    eprintln!("║  DO NOT add features = [\"bundled\"] to Cargo.toml.       ║");
    eprintln!("║  That recompiles all of DuckDB from C++ on every clean  ║");
    eprintln!("║  build and will peg every core on your machine.         ║");
    eprintln!("╚══════════════════════════════════════════════════════════╝");
    eprintln!();
    std::process::exit(1);
}
