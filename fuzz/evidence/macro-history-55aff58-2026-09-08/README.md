# Written macro history sanitizer check

A bounded AddressSanitizer campaign used an unchanged worktree at `55aff58`, including optional written macro definition history. The preprocessing harness checks recorded names, definition kinds, physical locations, and access spellings across all 80 initial comment, query, trigraph, scope, and history policy settings.

The campaign ran 402,876 inputs in 121.15 seconds, added 5,197 corpus entries, and reached peak RSS 483 MiB. It completed with no crash artifacts and unchanged source manifests. Leak detection was disabled. This is a bounded run, not a proof that all inputs are safe.

The report retains compiler and sanitizer identities, commands, limits, initial input bytes, source hashes, and raw logs. Header selection, formatting, derives, hard-link identity, and later macro value evaluation are outside this campaign’s source revision.
