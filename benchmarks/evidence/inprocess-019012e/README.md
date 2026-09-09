# Historical in-process benchmark

This directory retains the benchmark at `019012e`; its measurements are
summarized in [summary.json](summary.json). The driver and capture script are
historical evidence and contain paths from the measured machine.

The original manifest is retained byte for byte as
[driver/Cargo.toml.snapshot](driver/Cargo.toml.snapshot), with SHA256
`3d3adb7d2d81f587631f6bd55ec09c78a789aa416f9e3d40698a64eb82dc7175`.
Cargo scans files named `Cargo.toml` when resolving Git dependencies. Naming
this archived manifest as an active package let its absolute path dependency
redirect Toucan crates outside the pinned Git checkout.

To reproduce this historical driver, copy its files to a separate directory,
restore the manifest's name, and point its Toucan dependency at a checkout of
the recorded source. That environment needs its own source and toolchain audit;
the archived paths are not a portable build configuration.
