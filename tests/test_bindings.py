"""Real-file smoke test for the canonical Bruker Python record surface."""

from __future__ import annotations

import os
from pathlib import Path

import opentimstdf
import pytest


def test_canonical_records():
    path = Path(os.environ.get("OPENTIMSTDF_TEST_BUNDLE", "corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d"))
    if not path.is_dir():
        if os.environ.get("REQUIRE_CORPUS", "").strip() in {"1", "true", "all"}:
            pytest.fail(f"REQUIRE_CORPUS is set but {path} is not a TDF bundle")
        pytest.skip("set OPENTIMSTDF_TEST_BUNDLE to a TDF bundle")

    reader = opentimstdf.Reader(str(path))
    run = reader.run_info()
    assert run["source_file_name"] == path.name
    assert run["source_file_format"]["accession"]
    assert "opentimstdf.schema_version_major" in run["extra"]

    record = next(reader.iter_records())
    assert record["native_id"]
    assert len(record["mz"]) == len(record["intensity"])
    assert "opentimstdf.frame_id" in record["extra"]
    assert record["scan_number"] >= 1
    assert reader.frames()[0].polarity_symbol in {"+", "-"}

    for chrom in reader.read_chromatograms():
        assert len(chrom["time_sec"]) == len(chrom["intensity"])
