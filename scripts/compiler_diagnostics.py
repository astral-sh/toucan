"""Recognize compiler-driver failures before treating exit 1 as a source rejection."""


def has_crash_diagnostic(*streams: str) -> bool:
    # Drivers can report a crashed child with an ordinary diagnostic exit code.
    markers = (
        "internal compiler error",
        "please submit a bug report",
        "segmentation fault",
        "frontend command failed",
        "unable to execute command:",
        "please submit a full bug report",
        "llvm error:",
        "fatal error: error in backend",
        "fatal error: killed signal terminated program",
        "the compiler unexpectedly panicked",
        "panicked at",
        "assertion `",
    )
    for stream in streams:
        text = stream.lower()
        if any(marker in text for marker in markers):
            return True
    return False
