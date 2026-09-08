int warn(int value) __attribute__((warning("check value")));
void fail(void) __attribute__((__error__("unsupported call")));
int use(int value) { if (value < 0) fail(); return warn(value); }
