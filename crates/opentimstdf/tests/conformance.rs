//! Conformance harness: every spectrum from `TdfSource` must satisfy
//! the invariants in `openmassspec-core`.
//!
//! Looks for an extracted Bruker `.d` bundle in a few candidate locations
//! (see `bundle_dir()`) and skips silently when none is present, so a
//! plain checkout without any corpus stays green.
//!
//! In CI, `.github/workflows/ci.yml`'s `test` job downloads
//! `corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d` (repo-root-relative, Linux
//! leg only) ahead of `cargo test`, so this test exercises a real decode
//! path there instead of skipping - see Sigilweaver/OpenTimsTDF#35.

use std::path::PathBuf;

use openmassspec_core::conformance::assert_source_invariants;
use opentimstdf::mzml::TdfSource;

fn bundle_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Crate manifest is two levels deep under the OpenTimsTDF workspace root;
    // the cache/corpus dirs live at the workspace root.
    let candidates = [
        // CI / repo-root corpus dir (gitignored; populated by ci.yml). This
        // is the one CI actually uses.
        root.join("../../corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d"),
        // Local dev setups that have the full PRIDE cache checked out under
        // re/artifacts/cache/pride/...
        root.join("../../re/artifacts/cache/pride/PXD036417/NQO1-F107C_coi-N2-P_200-0C_3996.d"),
        root.join("../../re/artifacts/cache/pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d"),
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
    assert!(
        n > 0,
        "expected at least one spectrum from {}",
        dir.display()
    );
    eprintln!("opentimstdf: {n} spectra passed conformance");
}
