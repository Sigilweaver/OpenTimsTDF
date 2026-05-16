//! Minimal `.d/` frame-peak dumper.
//!
//! Usage: `cargo run --example dump -- <bundle.d> [frame_id]`

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: dump <bundle.d> [frame_id]");
        return ExitCode::from(2);
    }
    let bundle = &args[1];
    let frame_id: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);

    let reader = match opentdf::Reader::open(bundle) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("open failed: {e}");
            return ExitCode::from(1);
        }
    };
    println!("compression type: {}", reader.compression_type());

    let frame = match reader.frame(frame_id) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("frame {frame_id}: {e}");
            return ExitCode::from(1);
        }
    };
    println!(
        "frame {}  rt={:.3}s  scan_mode={}  msms_type={}  \
         num_scans={}  num_peaks={}  accumulation_time={:?}ms  summed={:?}",
        frame.id,
        frame.time,
        frame.scan_mode,
        frame.msms_type,
        frame.num_scans,
        frame.num_peaks,
        frame.accumulation_time,
        frame.summed_intensities,
    );

    let peaks = match reader.decode_peaks(&frame) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("decode: {e}");
            return ExitCode::from(1);
        }
    };

    let sum: u64 = peaks.iter().map(|p| u64::from(p.intensity)).sum();
    println!("decoded {} peaks, sum(intensity)={}", peaks.len(), sum);

    let calib = reader.calibration().ok();
    if let Some(c) = calib {
        println!(
            "calibration: mz = ({:.6} + {:.6e} * tof)^2   1/K0 = {:.6} + {:.6e} * scan",
            c.mz_intercept, c.mz_slope, c.im_intercept, c.im_slope
        );
    }
    for p in peaks.iter().take(10) {
        match calib {
            Some(c) => println!(
                "  scan={:>5} tof={:>8} intensity={:>6}  mz={:>10.4}  1/K0={:.4}",
                p.scan,
                p.tof,
                p.intensity,
                c.tof_to_mz(p.tof),
                c.scan_to_inv_mobility(p.scan),
            ),
            None => println!(
                "  scan={:>5} tof={:>8} intensity={:>6}",
                p.scan, p.tof, p.intensity
            ),
        }
    }
    if peaks.len() > 10 {
        println!("  ... ({} more)", peaks.len() - 10);
    }

    // Show isolation windows if available.
    if frame.msms_type != 0 {
        match reader.dia_windows_for_frame(frame_id) {
            Ok(Some(fw)) => {
                println!(
                    "diaPASEF windows (group {}): {} window(s)",
                    fw.window_group,
                    fw.windows.len()
                );
                for w in &fw.windows {
                    println!(
                        "  scans {}-{}  mz={:.2}+/-{:.2}  CE={:.1}eV",
                        w.scan_num_begin,
                        w.scan_num_end,
                        w.isolation_mz,
                        w.isolation_width / 2.0,
                        w.collision_energy,
                    );
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("dia windows: {e}"),
        }

        match reader.pasef_msms_info_for_frame(frame_id) {
            Ok(rows) if !rows.is_empty() => {
                println!("PASEF MS2 scan ranges: {} entry/entries", rows.len());
                for r in &rows {
                    println!(
                        "  scans {}-{}  mz={:.3}+/-{:.3}  CE={:.1}eV  precursor={}",
                        r.scan_num_begin,
                        r.scan_num_end,
                        r.isolation_mz,
                        r.isolation_width / 2.0,
                        r.collision_energy,
                        r.precursor_id,
                    );
                    if let Ok(Some(prec)) = reader.precursor(r.precursor_id) {
                        println!(
                            "    precursor: mz={:.4}  charge={:?}  intensity={:.0}  parent_frame={}",
                            prec.largest_peak_mz, prec.charge, prec.intensity, prec.parent_frame_id,
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("pasef msms info: {e}"),
        }
    }

    ExitCode::SUCCESS
}
