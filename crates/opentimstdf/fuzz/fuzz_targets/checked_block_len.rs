#![no_main]
//! Fuzz `checked_block_len`, the guard `decode_peaks_codec1`/
//! `decode_peaks_codec2` use to cap a `.tdf_bin`-derived allocation
//! (`block_size`, `bin_size`, or the codec-1 scan-offset table) against
//! the actual size of the file on disk before allocating. `file_len`,
//! `offset`, and `claimed_len` are all effectively attacker-controlled (the
//! latter two are arithmetic on bytes read straight from a corrupt/
//! adversarial frame block), so this must never panic, and it must never
//! accept a length that would read past the end of the file.

use libfuzzer_sys::fuzz_target;
use opentimstdf::checked_block_len;

fuzz_target!(|data: &[u8]| {
    if data.len() < 24 {
        return;
    }
    let file_len = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let offset = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let claimed_len = u64::from_le_bytes(data[16..24].try_into().unwrap());

    let result = checked_block_len(file_len, offset, claimed_len);

    match result {
        Some(len) => {
            // Any accepted length must be a real, in-bounds slice of the
            // file, and must round-trip back to claimed_len exactly.
            assert!(offset <= file_len);
            assert!(claimed_len <= file_len - offset);
            assert_eq!(len as u64, claimed_len);
        }
        None => {
            // Only reject when the read would truly overrun the file (or
            // claimed_len can't fit in usize on this target).
            let would_fit = offset <= file_len && claimed_len <= file_len - offset;
            assert!(!would_fit || usize::try_from(claimed_len).is_err());
        }
    }
});
