use crate::types::{Frame, Peak};

/// Map a Frames row to a `Frame` struct.
///
/// Column order must match the SELECT used in `Reader::frame()` and `Reader::frames()`:
/// Id, Time, NumScans, NumPeaks, TimsId, ScanMode, MsMsType,
/// MzCalibration, AccumulationTime, SummedIntensities
pub(crate) fn frame_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Frame> {
    Ok(Frame {
        id: row.get(0)?,
        time: row.get(1)?,
        num_scans: row.get(2)?,
        num_peaks: row.get(3)?,
        tims_id: row.get::<_, i64>(4)? as u64,
        scan_mode: row.get::<_, Option<i64>>(5)?.unwrap_or(0) as u32,
        msms_type: row.get::<_, Option<i64>>(6)?.unwrap_or(0) as u32,
        mz_calibration_id: row.get::<_, Option<i64>>(7)?.unwrap_or(1) as u32,
        accumulation_time: row.get(8)?,
        summed_intensities: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
    })
}

/// De-transpose and decode a codec-2 inner buffer. See SPEC §4.4.
pub fn decode_codec2(inner: &[u8], num_scans: u32, num_peaks: u32) -> Vec<Peak> {
    let dsints = num_scans as usize + 2 * num_peaks as usize;
    debug_assert_eq!(inner.len(), 4 * dsints);

    let b0 = &inner[0..dsints];
    let b1 = &inner[dsints..2 * dsints];
    let b2 = &inner[2 * dsints..3 * dsints];
    let b3 = &inner[3 * dsints..4 * dsints];

    let mut logical = Vec::with_capacity(dsints);
    for i in 0..dsints {
        let v = u32::from(b0[i])
            | (u32::from(b1[i]) << 8)
            | (u32::from(b2[i]) << 16)
            | (u32::from(b3[i]) << 24);
        logical.push(v);
    }

    let ns = num_scans as usize;
    let np = num_peaks as usize;
    let (header, peak_stream) = logical.split_at(ns);

    let mut peaks = Vec::with_capacity(np);
    let mut read: usize = 0;

    if ns >= 2 {
        for scan in 0..(ns - 1) {
            let peaks_in_scan = (header[scan + 1] / 2) as usize;
            let mut accum: u32 = u32::MAX;
            for _ in 0..peaks_in_scan {
                accum = accum.wrapping_add(peak_stream[read]);
                read += 1;
                let intensity = peak_stream[read];
                read += 1;
                peaks.push(Peak {
                    scan: scan as u32,
                    tof: accum,
                    intensity,
                });
            }
        }
    }

    let last_scan = ns.saturating_sub(1) as u32;
    let mut accum: u32 = u32::MAX;
    while peaks.len() < np {
        accum = accum.wrapping_add(peak_stream[read]);
        read += 1;
        let intensity = peak_stream[read];
        read += 1;
        peaks.push(Peak {
            scan: last_scan,
            tof: accum,
            intensity,
        });
    }

    peaks
}

/// Decode a codec-1 ("legacy packed") frame payload. See SPEC §4.5.
///
/// `compressed` is the tail of the frame after the 8-byte header and the
/// `(scan_count+1)` u32 scan offset table. `scan_offsets` is that offset
/// table, already rebased so `scan_offsets[i]` is a byte index into
/// `compressed` where scan `i`'s LZF blob starts.
///
/// Per-scan: LZF-decompress, interpret the bytes as little-endian `i32`
/// values, and walk them with the rule:
/// * positive value → emit one peak `(scan, tof, value)`, and if the
///   previous value was also an intensity, bump `tof += 1`.
/// * negative value → advance `tof` by `-value`.
///
/// Ported from alphatims' `parse_decompressed_bruker_binary_type1`.
pub fn decode_codec1(
    compressed: &[u8],
    scan_offsets: &[usize],
    num_peaks: u32,
    max_peaks_per_scan: usize,
) -> std::result::Result<Vec<Peak>, String> {
    let scan_count = scan_offsets.len().saturating_sub(1);
    let mut peaks: Vec<Peak> = Vec::with_capacity(num_peaks as usize);
    let scratch_cap = max_peaks_per_scan.saturating_mul(4 * 2).max(64);
    let mut scratch = Vec::with_capacity(scratch_cap);

    for scan in 0..scan_count {
        let start = scan_offsets[scan];
        let end = scan_offsets[scan + 1];
        if start == end {
            continue;
        }
        if end > compressed.len() || start > end {
            return Err(format!(
                "scan {scan} offsets [{start}..{end}) out of compressed slice (len={})",
                compressed.len()
            ));
        }
        scratch.clear();
        lzf_decompress(&compressed[start..end], &mut scratch)
            .map_err(|e| format!("scan {scan}: lzf: {e}"))?;

        if scratch.len() % 4 != 0 {
            return Err(format!(
                "scan {scan}: decompressed size {} not a multiple of 4",
                scratch.len()
            ));
        }
        let mut tof: u32 = 0;
        let mut prev_was_intensity = true;
        for chunk in scratch.chunks_exact(4) {
            // chunks_exact(4) guarantees chunk.len() == 4
            #[allow(clippy::unwrap_used)]
            let v = i32::from_le_bytes(chunk.try_into().unwrap());
            if v >= 0 {
                if prev_was_intensity {
                    tof = tof.wrapping_add(1);
                }
                peaks.push(Peak {
                    scan: scan as u32,
                    tof: tof.wrapping_sub(1),
                    intensity: v as u32,
                });
                prev_was_intensity = true;
            } else {
                tof = tof.wrapping_add((-v) as u32);
                prev_was_intensity = false;
            }
        }
    }

    Ok(peaks)
}

/// Minimal LZF (libLZF) decompressor - streaming, allocation-free beyond
/// the `out` vec. Format reference: <http://oldhome.schmorp.de/marc/liblzf.html>.
pub(crate) fn lzf_decompress(
    input: &[u8],
    out: &mut Vec<u8>,
) -> std::result::Result<(), &'static str> {
    let mut ip = 0;
    while ip < input.len() {
        let ctrl = input[ip];
        ip += 1;
        if ctrl < 32 {
            let run = ctrl as usize + 1;
            if ip + run > input.len() {
                return Err("literal run overruns input");
            }
            out.extend_from_slice(&input[ip..ip + run]);
            ip += run;
        } else {
            let mut length = (ctrl >> 5) as usize;
            if length == 7 {
                if ip >= input.len() {
                    return Err("ref length byte missing");
                }
                length += input[ip] as usize;
                ip += 1;
            }
            length += 2;
            if ip >= input.len() {
                return Err("ref low byte missing");
            }
            let ref_distance = (((ctrl & 0x1f) as usize) << 8) | input[ip] as usize;
            ip += 1;
            let ref_pos = out
                .len()
                .checked_sub(ref_distance + 1)
                .ok_or("ref distance precedes output start")?;
            for i in 0..length {
                let byte = out[ref_pos + i];
                out.push(byte);
            }
        }
    }
    Ok(())
}
