# Toucan allocator measurements

5 processes per variant/workload; 136 frozen input files.

Timings are medians of process medians. Allocator changes compare the same frontend source against System. Brackets show observed paired-round ranges, not confidence intervals. Negative changes mean smaller values.

RSS covers the entire process, including output verification and checked-graph JSON serialization. Allocation counts are not collected in this study. These local results make no CI or statistical-significance claim.

- baseline: `0a47f9781d92746bb97184a447aea5965b9dafb9`
- optimized: `4228eeee8c345b7b7477a76dc45c86082d6ba3b3`
  Production crates and workspace dependencies match `2010e8d2aa0dfa71da3d6fa7ab07a1ccd2fd0ff3`; the measured commit adds only benchmark allocator selection.

## zlib / toucan-builder

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 21.597 | — | 11.15 [10.90, 11.15] | — |
| baseline | jemalloc | 19.540 | -8.64% [-10.75, -3.96] | 13.25 [13.00, 13.50] | +18.81% [+16.57, +23.83] |
| baseline | mimalloc | 18.904 | -12.58% [-13.65, -11.61] | 24.23 [23.80, 24.23] | +117.27% [+113.42, +122.06] |
| optimized | system | 18.005 | — | 11.00 [10.79, 11.05] | — |
| optimized | jemalloc | 16.817 | -6.60% [-7.91, -4.76] | 12.25 [12.00, 12.25] | +11.18% [+8.63, +13.54] |
| optimized | mimalloc | 16.125 | -10.36% [-14.30, -8.02] | 24.48 [24.23, 24.48] | +121.57% [+119.54, +126.86] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -17.47% [-17.79, -12.88] | -1.00% [-3.26, +1.15] |
| jemalloc | -14.52% [-20.05, -12.10] | -7.55% [-11.11, -5.77] |
| mimalloc | -14.22% [-15.38, -12.87] | +1.02% [+0.00, +2.86] |

## sqlite / toucan-builder

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 25.495 | — | 15.36 [15.09, 15.39] | — |
| baseline | jemalloc | 22.613 | -11.31% [-15.86, -9.42] | 17.00 [16.75, 17.00] | +10.51% [+9.05, +12.66] |
| baseline | mimalloc | 21.752 | -14.57% [-20.70, -7.49] | 28.78 [28.73, 29.78] | +87.81% [+87.08, +93.50] |
| optimized | system | 21.336 | — | 15.42 [15.31, 15.58] | — |
| optimized | jemalloc | 18.055 | -15.38% [-17.59, -7.71] | 15.25 [15.00, 15.50] | -0.51% [-2.04, +0.51] |
| optimized | mimalloc | 17.287 | -18.41% [-21.45, -11.92] | 30.25 [30.25, 30.50] | +96.15% [+94.16, +98.98] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -16.44% [-27.13, -13.30] | +0.84% [-0.20, +2.20] |
| jemalloc | -20.06% [-20.80, -18.85] | -8.96% [-11.76, -8.82] |
| mimalloc | -20.53% [-25.69, -19.06] | +5.09% [+1.56, +6.14] |

## zstd / toucan-builder

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 8.955 | — | 10.15 [9.98, 10.43] | — |
| baseline | jemalloc | 7.860 | -12.06% [-28.78, -10.85] | 11.75 [11.75, 12.00] | +15.78% [+12.70, +20.23] |
| baseline | mimalloc | 7.384 | -17.65% [-23.54, -14.94] | 21.98 [21.98, 22.48] | +119.01% [+110.79, +125.21] |
| optimized | system | 7.362 | — | 10.12 [10.09, 10.36] | — |
| optimized | jemalloc | 6.462 | -11.61% [-12.40, -7.75] | 11.75 [11.50, 12.00] | +16.41% [+13.42, +18.66] |
| optimized | mimalloc | 5.999 | -18.51% [-18.80, -13.19] | 24.22 [23.97, 24.47] | +139.38% [+131.37, +141.67] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -18.18% [-37.80, -16.00] | +1.37% [-3.18, +2.08] |
| jemalloc | -17.08% [-19.44, -15.43] | +0.00% [-2.13, +2.13] |
| mimalloc | -18.31% [-29.38, -17.46] | +9.58% [+7.75, +11.34] |

## libgit2 / toucan-builder

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 194.519 | — | 39.00 [38.75, 39.01] | — |
| baseline | jemalloc | 175.267 | -9.43% [-10.60, -7.05] | 45.70 [45.21, 49.32] | +17.96% [+15.88, +26.48] |
| baseline | mimalloc | 169.376 | -11.59% [-15.64, -9.89] | 64.85 [53.63, 64.96] | +66.30% [+38.41, +67.66] |
| optimized | system | 157.969 | — | 38.89 [38.87, 39.05] | — |
| optimized | jemalloc | 145.206 | -8.63% [-12.72, -7.31] | 44.96 [43.09, 47.20] | +15.66% [+10.62, +21.15] |
| optimized | mimalloc | 140.311 | -10.34% [-14.42, +0.59] | 62.72 [60.55, 63.20] | +61.00% [+55.79, +62.49] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -18.21% [-19.66, -14.45] | -0.14% [-0.34, +0.78] |
| jemalloc | -17.83% [-18.63, -16.80] | -0.98% [-12.62, +4.40] |
| mimalloc | -17.14% [-17.64, -2.66] | -2.72% [-6.63, +17.42] |

## libgit2 / checked

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 168.981 | — | 153.99 [153.91, 154.30] | — |
| baseline | jemalloc | 157.134 | -6.35% [-9.66, -4.10] | 176.54 [163.95, 183.28] | +14.64% [+6.47, +18.78] |
| baseline | mimalloc | 167.474 | -1.23% [-6.36, +4.86] | 155.64 [155.57, 156.55] | +1.09% [+0.83, +1.62] |
| optimized | system | 135.403 | — | 153.27 [153.00, 153.58] | — |
| optimized | jemalloc | 129.936 | -4.04% [-5.32, -2.02] | 179.50 [179.42, 182.86] | +17.32% [+17.07, +19.06] |
| optimized | mimalloc | 133.298 | -2.22% [-3.94, +3.15] | 158.65 [149.15, 163.94] | +3.30% [-2.65, +7.15] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -20.02% [-24.31, -16.73] | -0.51% [-0.84, -0.22] |
| jemalloc | -17.75% [-19.34, -16.73] | +1.67% [-2.06, +9.67] |
| mimalloc | -21.10% [-23.08, -16.62] | +1.97% [-4.17, +5.38] |

## zlib / checked

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 21.325 | — | 26.22 [25.94, 26.25] | — |
| baseline | jemalloc | 19.205 | -9.35% [-12.28, -5.18] | 60.75 [52.64, 60.75] | +131.46% [+102.94, +133.86] |
| baseline | mimalloc | 19.108 | -10.40% [-14.01, -8.38] | 49.20 [49.20, 49.22] | +87.63% [+87.44, +89.73] |
| optimized | system | 17.263 | — | 26.14 [25.91, 26.16] | — |
| optimized | jemalloc | 16.908 | -2.44% [-6.26, +0.32] | 59.36 [56.68, 66.98] | +126.87% [+118.67, +156.28] |
| optimized | mimalloc | 16.265 | -6.38% [-8.66, -1.36] | 49.20 [49.20, 49.22] | +88.22% [+88.05, +89.90] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -19.36% [-21.24, -16.50] | -0.27% [-1.24, +0.84] |
| jemalloc | -12.84% [-16.19, -11.51] | -0.73% [-2.30, +10.48] |
| mimalloc | -14.41% [-17.59, -11.19] | +0.00% [-0.05, +0.05] |

## zlib-adler32 / checked

| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |
|---|---|---:|---:|---:|---:|
| baseline | system | 37.967 | — | 41.27 [41.14, 41.32] | — |
| baseline | jemalloc | 34.933 | -8.33% [-13.02, -6.28] | 74.71 [72.84, 80.97] | +81.60% [+76.29, +96.19] |
| baseline | mimalloc | 38.001 | -0.19% [-1.73, +29.06] | 63.93 [55.85, 81.54] | +54.90% [+35.16, +97.59] |
| optimized | system | 31.509 | — | 40.79 [40.71, 40.80] | — |
| optimized | jemalloc | 30.289 | -3.18% [-5.13, -1.74] | 72.65 [66.71, 72.67] | +78.07% [+63.54, +78.43] |
| optimized | mimalloc | 32.466 | +2.83% [+2.34, +10.36] | 65.22 [59.57, 83.02] | +59.88% [+46.05, +103.60] |

Frontend changes from baseline to optimized with allocator held fixed:

| Allocator | Time change % [range] | Process RSS change % [range] |
|---|---:|---:|
| system | -17.33% [-18.52, -16.25] | -1.15% [-1.34, -0.85] |
| jemalloc | -12.90% [-13.97, -8.65] | -10.28% [-12.16, -0.24] |
| mimalloc | -15.14% [-33.52, -7.65] | +7.11% [-7.16, +29.67] |

