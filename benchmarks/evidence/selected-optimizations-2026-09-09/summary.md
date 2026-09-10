# Toucan selected optimization measurements

Complete captures: 5 timing processes and 3 allocation processes per variant/workload; 136 frozen input files.

Timings are medians of process medians. Changes pair the same round; brackets show the observed minimum and maximum percentage changes, not confidence intervals. Negative changes mean smaller values. The selected variant contains only the six authorized optimizations; both comparisons use the fresh baseline.

RSS is the whole benchmark process, including verification and checked-graph JSON serialization. Requested allocation bytes are cumulative requests, including reallocations; they are not live or peak heap usage.

## zlib / toucan-builder

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 22.164 | — | — | 10.88 [10.75, 10.93] |
| selected | 17.680 | -16.65% [-21.03, -4.01] | -16.65% [-21.03, -4.01] | 11.00 [10.75, 11.25] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 78,357 | — | — | 17.716 | — | — |
| selected | 42,364 | -45.93% | -45.93% | 16.074 | -9.27% | -9.27% |

## sqlite / toucan-builder

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 27.702 | — | — | 15.36 [14.95, 15.39] |
| selected | 21.419 | -24.47% [-28.58, -15.12] | -24.47% [-28.58, -15.12] | 15.61 [15.56, 15.78] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 72,361 | — | — | 22.471 | — | — |
| selected | 59,659 | -17.55% | -17.55% | 21.216 | -5.58% | -5.58% |

## zstd / toucan-builder

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 9.219 | — | — | 10.36 [10.18, 10.60] |
| selected | 7.595 | -15.24% [-28.45, -14.70] | -15.24% [-28.45, -14.70] | 10.41 [10.29, 10.49] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 28,636 | — | — | 7.633 | — | — |
| selected | 20,267 | -29.23% | -29.23% | 7.087 | -7.15% | -7.15% |

## libgit2 / toucan-builder

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 196.233 | — | — | 38.78 [38.75, 39.04] |
| selected | 158.890 | -16.31% [-20.89, -5.71] | -16.31% [-20.89, -5.71] | 38.63 [38.52, 38.77] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 552,965 | — | — | 162.756 | — | — |
| selected | 327,345 | -40.80% | -40.80% | 150.828 | -7.33% | -7.33% |

## libgit2 / checked

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 170.073 | — | — | 153.92 [153.52, 153.96] |
| selected | 145.930 | -15.87% [-20.28, -10.76] | -15.87% [-20.28, -10.76] | 153.72 [153.55, 154.04] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 481,769 | — | — | 137.102 | — | — |
| selected | 256,149 | -46.83% | -46.83% | 125.173 | -8.70% | -8.70% |

## zlib / checked

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 21.938 | — | — | 25.99 [25.97, 26.00] |
| selected | 17.723 | -19.10% [-43.97, -16.33] | -19.10% [-43.97, -16.33] | 26.03 [26.00, 26.25] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 74,224 | — | — | 18.181 | — | — |
| selected | 38,231 | -48.49% | -48.49% | 16.539 | -9.03% | -9.03% |

## zlib-adler32 / checked

| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |
|---|---:|---:|---:|---:|
| baseline | 37.919 | — | — | 41.11 [41.06, 41.15] |
| selected | 31.994 | -15.45% [-17.31, -14.50] | -15.45% [-17.31, -14.50] | 41.16 [40.91, 41.30] |

| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 147,514 | — | — | 35.086 | — | — |
| selected | 71,785 | -51.34% | -51.34% | 31.859 | -9.20% | -9.20% |

