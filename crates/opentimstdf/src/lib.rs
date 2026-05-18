//! OpenTimsTDF - Rust reader for timsTOF `.d/` (TDF) mass spectrometry bundles.
//!
//! The format and codecs are documented in `re/SPEC.md` (and mirrored on
//! the docs site). Both compression codecs are supported:
//!
//! * **Codec 2** (`TimsCompressionType == 2`) - byte-transposed delta-TOF over
//!   zstd. Used by modern acquisitions.
//! * **Codec 1** (`TimsCompressionType == 1`) - per-scan LZF blobs with a
//!   signed-int32 delta stream. Used by older acquisitions (e.g. PXD022216).

pub mod calibration;
pub mod codec;
pub mod error;
pub mod mzml;
pub mod reader;
pub mod types;

pub use calibration::Calibration;
pub use codec::{decode_codec1, decode_codec2};
pub use error::{Error, Result};
pub use reader::Reader;
pub use types::{
    DiaFrameWindows, DiaWindow, Frame, Metadata, PasefMsMsInfo, Peak, Precursor, PrmMsMsInfo,
    PrmTarget,
};
