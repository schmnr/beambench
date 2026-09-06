# Beam Bench dependency patch

Source: crates.io u-nesting-d2 0.3.1, upstream commit `55cae07ed000b17a1523fa8df93cd045d91f28b9`.
Upstream: https://github.com/iyulab/U-Nesting

The normalized Cargo manifest changes i_overlay from 1.9 to ~4.0.6, sharing
the geometry version used by Beam Bench. The two overlay calls in src/nfp.rs
use the new custom constructor to retain the previous clockwise outer contours.
Workspace rustfmt also adjusts indentation in two existing nester.rs tests.
All other source, tests, and the benchmark are unchanged. This removes i_tree versions affected by RUSTSEC-2025-0165.
The upstream license is included in LICENSE.

The current upstream 0.9.0 release still requires i_overlay 1.9. Remove this local
patch when an upstream release supports the corrected dependency and its API and
nesting behavior have been validated with Beam Bench.
