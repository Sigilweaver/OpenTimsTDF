#![no_main]
//! Fuzz the codec-2 peak decoder. `decode_codec2` takes an arbitrary byte
//! buffer plus scan/peak counts straight from a (potentially corrupt) TDF
//! frame, so it must be total: no input may panic the host process.
//!
//! The harness picks small scan/peak counts, then builds an `inner` buffer
//! of exactly the length the decoder requires (cycling the remaining fuzz
//! bytes) so the decode body runs on adversarial header and stream content,
//! not just the length guard.

use libfuzzer_sys::fuzz_target;
use opentimstdf::decode_codec2;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let num_scans = (data[0] as u32) & 0x3f; // 0..=63
    let num_peaks = data[1] as u32; // 0..=255
    let need = 4 * (num_scans as usize + 2 * num_peaks as usize);

    let tail = &data[2..];
    let inner: Vec<u8> = if tail.is_empty() {
        vec![0u8; need]
    } else {
        (0..need).map(|i| tail[i % tail.len()]).collect()
    };

    let _ = decode_codec2(&inner, num_scans, num_peaks);
});
