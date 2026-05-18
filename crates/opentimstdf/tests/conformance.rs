//! Conformance harness: every spectrum from `TdfSource` must satisfy
//! the invariants in `openproteo-core`.
//!
//! Uses the same env var convention as the other tests in this crate:
//! looks under `re/artifacts/cache/pride/...` for an extracted bundle
//! and skips silently when absent.

use std::path::PathBuf;

use openproteo_core::conformance::assert_source_invariants;
use opentimstdf::mzml::TdfSource;

fn bundle_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Crate manifest is two levels deep under the OpenTDF workspace root;
    // the cache lives at the workspace root.
    let cache = root.join("../../re/artifacts/cache/pride");
    let candidates = [
        cache.join("PXD036417/NQO1-F107C_coi-N2-P_200-0C_3996.d"),
        cache.join("PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d"),
    ];
    candidates
        .into_iter()
        .find(|p| p.join("analysis.tdf").exists() && p.join("analysis.tdf_bin").exists())
}

#[test]
fn opentimstdf_conformance() {
    let Some(dir) = bundle_dir() else {
        eprintln!("skipping: no Bruker TDF cache present");
        return;
    };
    let mut src = TdfSource::open(&dir).expect("open bundle");
    let n = assert_source_invariants(&mut src).expect("conformance");
    assert!(n > 0, "expected at least one spectrum from {}", dir.display());
    eprintln!("opentimstdf: {n} spectra passed conformance");
}
