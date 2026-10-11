//! Convert a Bruker timsTOF `.d/` bundle to mzML.
//!
//! Usage:
//!
//! ```text
//! cargo run --example to_mzml --release -- path/to/bundle.d out.mzML [--indexed]
//! ```

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: to_mzml <bundle.d> <out.mzML> [--indexed]");
        std::process::exit(2);
    }
    let bundle = &args[1];
    let out_path = &args[2];
    let indexed = args.iter().any(|a| a == "--indexed");

    let t0 = Instant::now();
    let f = File::create(out_path)?;
    let mut w = BufWriter::new(f);
    let written = if indexed {
        opentimstdf::mzml::write_indexed_mzml(bundle, &mut w)
    } else {
        opentimstdf::mzml::write_mzml(bundle, &mut w)
    }
    .map_err(Box::<dyn std::error::Error>::from)
    .and_then(|summary| {
        w.flush()?;
        Ok(summary)
    });
    let summary = match written {
        Ok(s) => s,
        Err(e) => {
            // Do not leave a truncated mzML behind that looks like output.
            drop(w);
            if let Err(rm) = std::fs::remove_file(out_path) {
                eprintln!("could not remove partial output {out_path}: {rm}");
            }
            return Err(e);
        }
    };
    let dt = t0.elapsed();
    let tag = if indexed { " (indexed)" } else { "" };
    eprintln!(
        "Wrote {out_path} ({} spectra, {} chromatograms) in {:.1}s{tag}",
        summary.spectra_written,
        summary.chromatograms_written,
        dt.as_secs_f64()
    );
    Ok(())
}
