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

/// Caps a claimed read length against what a file could actually contain,
/// without panicking on any input (including on-disk lengths chosen
/// adversarially to overflow the arithmetic below).
///
/// `file_len` is the total size of the file; `offset` is where the read
/// would start; `claimed_len` is a length read from the file itself (e.g.
/// a codec-2 `block_size` or codec-1 `bin_size` derived value) that the
/// caller is about to `Vec::with_capacity` and read into. Returns
/// `Some(claimed_len as usize)` only if the read would stay within the
/// file; otherwise `None`, which the caller should treat as a corrupt or
/// adversarial frame rather than allocating on the strength of unverified
/// on-disk data.
pub fn checked_block_len(file_len: u64, offset: u64, claimed_len: u64) -> Option<usize> {
    let remaining = file_len.checked_sub(offset)?;
    if claimed_len > remaining {
        return None;
    }
    usize::try_from(claimed_len).ok()
}

/// De-transpose and decode a codec-2 inner buffer. See SPEC §4.4.
///
/// This is total: any `inner`, `num_scans`, `num_peaks` combination returns
/// without panicking. `Reader::decode_peaks` validates that
/// `inner.len() == 4 * (num_scans + 2 * num_peaks)` before calling, but a
/// corrupt frame can still carry header scan-lengths that over-run the peak
/// stream even with a correctly sized buffer, so every stream read is
/// bounds-checked and decoding stops cleanly when the stream is exhausted.
pub fn decode_codec2(inner: &[u8], num_scans: u32, num_peaks: u32) -> Vec<Peak> {
    let dsints = num_scans as usize + 2 * num_peaks as usize;
    // Guard the de-transpose slicing directly so malformed input to this
    // public function cannot panic.
    if inner.len() != 4 * dsints {
        return Vec::new();
    }

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
        'scans: for scan in 0..(ns - 1) {
            let peaks_in_scan = (header[scan + 1] / 2) as usize;
            let mut accum: u32 = u32::MAX;
            for _ in 0..peaks_in_scan {
                let (Some(&delta), Some(&intensity)) =
                    (peak_stream.get(read), peak_stream.get(read + 1))
                else {
                    break 'scans;
                };
                accum = accum.wrapping_add(delta);
                read += 2;
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
        let (Some(&delta), Some(&intensity)) = (peak_stream.get(read), peak_stream.get(read + 1))
        else {
            break;
        };
        accum = accum.wrapping_add(delta);
        read += 2;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_block_len_accepts_len_within_file() {
        assert_eq!(checked_block_len(100, 10, 90), Some(90));
        assert_eq!(checked_block_len(100, 10, 0), Some(0));
        assert_eq!(checked_block_len(100, 100, 0), Some(0));
    }

    #[test]
    fn checked_block_len_rejects_len_past_end_of_file() {
        assert_eq!(checked_block_len(100, 10, 91), None);
        // Offset already past the end of the file.
        assert_eq!(checked_block_len(100, 101, 0), None);
        // Would have wrapped a naive u64 subtraction.
        assert_eq!(checked_block_len(10, u64::MAX, 1), None);
    }

    #[test]
    fn checked_block_len_never_panics_on_adversarial_input() {
        // Values chosen to stress every overflow-prone combination without
        // an exhaustive fuzz run (see fuzz_targets/checked_block_len.rs).
        for file_len in [0u64, 1, u64::MAX / 2, u64::MAX] {
            for offset in [0u64, 1, u64::MAX / 2, u64::MAX] {
                for claimed_len in [0u64, 1, u64::MAX / 2, u64::MAX] {
                    let _ = checked_block_len(file_len, offset, claimed_len);
                }
            }
        }
    }

    /// Build a codec-2 `inner` buffer from a logical u32 sequence by
    /// byte-plane transposition (the inverse of the decoder's de-transpose).
    fn build_inner(logical: &[u32]) -> Vec<u8> {
        let n = logical.len();
        let mut inner = vec![0u8; 4 * n];
        for (i, &v) in logical.iter().enumerate() {
            inner[i] = (v & 0xff) as u8;
            inner[n + i] = ((v >> 8) & 0xff) as u8;
            inner[2 * n + i] = ((v >> 16) & 0xff) as u8;
            inner[3 * n + i] = ((v >> 24) & 0xff) as u8;
        }
        inner
    }

    #[test]
    fn decode_codec2_wrong_length_returns_empty() {
        // dsints = 3 + 2*2 = 7, so the expected buffer is 28 bytes.
        assert!(decode_codec2(&[], 3, 2).is_empty());
        assert!(decode_codec2(&[0u8; 27], 3, 2).is_empty());
    }

    #[test]
    fn decode_codec2_corrupt_header_does_not_panic() {
        // Correct length (28 bytes) but header scan-lengths claim far more
        // peaks than the stream holds. Must stop cleanly, not panic.
        let logical = [0u32, 1000, 1000, 1, 100, 5, 200];
        let inner = build_inner(&logical);
        let peaks = decode_codec2(&inner, 3, 2);
        assert!(peaks.len() <= 2);
    }

    #[test]
    fn decode_codec2_valid_decode() {
        // ns=3, np=2. header[1]=header[2]=2 -> one peak each for scans 0,1.
        // peak_stream = [delta, intensity, delta, intensity].
        // accum starts at u32::MAX; +1 wraps to 0, +5 wraps to 4.
        let logical = [0u32, 2, 2, 1, 100, 5, 200];
        let inner = build_inner(&logical);
        let peaks = decode_codec2(&inner, 3, 2);
        assert_eq!(peaks.len(), 2);
        assert_eq!(
            (peaks[0].scan, peaks[0].tof, peaks[0].intensity),
            (0, 0, 100)
        );
        assert_eq!(
            (peaks[1].scan, peaks[1].tof, peaks[1].intensity),
            (1, 4, 200)
        );
    }
}
