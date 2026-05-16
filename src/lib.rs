//! OpenTDF — reverse-engineered parser for Bruker timsTOF `.d/` (TDF)
//! mass spectrometry bundles.
//!
//! The format spec lives in `re/SPEC.md`; each hypothesis is derived
//! and verified in `re/JOURNAL.md`.
//!
//! Both codecs are supported:
//! * **Codec 2** (`TimsCompressionType == 2`) — byte-transposed delta-TOF over
//!   zstd. Used by all modern timsTOF acquisitions.
//! * **Codec 1** (`TimsCompressionType == 1`) — per-scan LZF blobs with a
//!   signed-int32 delta stream. Used by older acquisitions (e.g. PXD022216).

pub mod calibration;
pub mod codec;
pub mod error;
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
