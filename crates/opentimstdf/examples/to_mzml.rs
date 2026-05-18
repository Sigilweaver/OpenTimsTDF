//! Convert a Bruker timsTOF `.d/` bundle to mzML.
//!
//! Usage:
//!
//! ```text
//! cargo run --example to_mzml --release -- path/to/bundle.d out.mzML [--indexed]
//! ```

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    if indexed {
        opentimstdf::mzml::write_indexed_mzml(bundle, &mut w)?;
    } else {
        opentimstdf::mzml::write_mzml(bundle, &mut w)?;
    }
    let dt = t0.elapsed();
    let tag = if indexed { " (indexed)" } else { "" };
    eprintln!("Wrote {out_path} in {:.1}s{tag}", dt.as_secs_f64());
    Ok(())
}
