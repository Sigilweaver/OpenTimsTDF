//! Shared corpus lookup for the integration tests.
//!
//! Paths resolve from the workspace root (two levels above this crate's
//! manifest), not from the crate directory, so `corpus/` and
//! `re/artifacts/cache/` at the repo root are found.
//!
//! Environment:
//!
//! * `OPENTIMSTDF_TEST_BUNDLE` - path to one extracted `.d` bundle used by
//!   the bundle-agnostic tests. Relative paths resolve from the repo root.
//!   Default: `corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d` (the bundle CI
//!   downloads), then the PXD027359 bundle in the PRIDE cache.
//! * `OPENTIMSTDF_TEST_CACHE` - root of the PRIDE cache used by the
//!   bundle-specific tests. Default: `re/artifacts/cache` under the repo root.
//! * `REQUIRE_CORPUS` - `1` makes the bundle-agnostic tests fail instead of
//!   skip when no bundle is found (set in CI on the leg that downloads the
//!   corpus). `all` additionally makes the bundle-specific PRIDE tests fail
//!   when their bundle is missing.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub const PXD027359: &str =
    "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d";
const CI_BUNDLE: &str = "corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d";

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn resolve(p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root().join(p)
    }
}

fn require_level() -> u8 {
    match std::env::var("REQUIRE_CORPUS").as_deref().map(str::trim) {
        Ok("all") => 2,
        Ok("1") | Ok("true") => 1,
        _ => 0,
    }
}

fn is_bundle(p: &Path) -> bool {
    p.join("analysis.tdf").is_file() && p.join("analysis.tdf_bin").is_file()
}

fn cache_root() -> PathBuf {
    match std::env::var("OPENTIMSTDF_TEST_CACHE") {
        Ok(v) if !v.trim().is_empty() => resolve(v.trim()),
        _ => repo_root().join("re/artifacts/cache"),
    }
}

/// Skip (return `None`) or panic, depending on `REQUIRE_CORPUS`.
fn missing(level_needed: u8, what: &str) -> Option<PathBuf> {
    if require_level() >= level_needed {
        panic!("REQUIRE_CORPUS is set but {what} is not present");
    }
    eprintln!("skipping: {what} not present");
    None
}

/// Generic bundle for tests that make no bundle-specific assertions.
pub fn test_bundle() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("OPENTIMSTDF_TEST_BUNDLE") {
        if !v.trim().is_empty() {
            let p = resolve(v.trim());
            assert!(
                is_bundle(&p),
                "OPENTIMSTDF_TEST_BUNDLE={v} does not point to a .d bundle ({})",
                p.display()
            );
            return Some(p);
        }
    }
    let candidates = [
        resolve(CI_BUNDLE),
        cache_root().join("pride/PXD036417/NQO1-F107C_coi-N2-P_200-0C_3996.d"),
        cache_root().join(PXD027359),
    ];
    if let Some(p) = candidates.into_iter().find(|p| is_bundle(p)) {
        return Some(p);
    }
    missing(1, "test bundle (set OPENTIMSTDF_TEST_BUNDLE)")
}

/// A specific PRIDE bundle under the cache root (`rel` is relative to it).
pub fn pride_bundle(rel: &str) -> Option<PathBuf> {
    let p = cache_root().join(rel);
    if is_bundle(&p) {
        Some(p)
    } else {
        missing(2, &format!("PRIDE bundle {}", p.display()))
    }
}

/// Metadata-only probe (`analysis.tdf` without `analysis.tdf_bin`) under
/// `corpus/probes/<accession>/` at the repo root.
pub fn probe_dir(accession: &str) -> Option<PathBuf> {
    let p = repo_root().join("corpus/probes").join(accession);
    if p.join("analysis.tdf").is_file() {
        Some(p)
    } else {
        missing(2, &format!("probe corpus {}", p.display()))
    }
}
