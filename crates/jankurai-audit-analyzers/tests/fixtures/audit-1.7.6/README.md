# Real auditor fixture

Captured 2026-11-01 by running `PATH=/usr/local/libexec/jain:$PATH bash ops/ci/audit.sh`
on the Run13 repair working tree (cherry-picks ff6b80e and 67bac6d plus the removal
of the Python scope filter and documentation corrections). `auditor.txt` records
the actual 1.7.6 executable digest. No report fields have been rewritten.
The lane exited 1: score 72, raw 92, two HLT-005 findings remain. The report and
queue prove wire-format compatibility and flag support, not a passing audit.
The older committed ratchet baseline is unchanged.
