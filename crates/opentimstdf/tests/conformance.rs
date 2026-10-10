//! Conformance harness: every spectrum from `TdfSource` must satisfy
//! the invariants in `openmassspec-core`.
//!
//! Uses `common::test_bundle()` (honors `OPENTIMSTDF_TEST_BUNDLE`) and
//! skips when no bundle is present, so a plain checkout stays green, unless
//! `REQUIRE_CORPUS=1` is set, in which case a missing bundle fails.
//!
//! In CI, `.github/workflows/ci.yml`'s `test` job downloads
//! `corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d` (repo-root-relative, Linux
//! leg only) ahead of `cargo test`, so this test exercises a real decode
//! path there instead of skipping - see Sigilweaver/OpenTimsTDF#35.

mod common;

use openmassspec_core::conformance::assert_source_invariants;
use opentimstdf::mzml::TdfSource;

#[test]
fn opentimstdf_conformance() {
    let Some(dir) = common::test_bundle() else {
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
