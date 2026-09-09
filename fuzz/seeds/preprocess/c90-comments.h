int division = 6 //**/ 2;
#define UNUSED 1 // enable Clang's C90 comment extension
#define AFTER 6 //**/ 2
#if 0
// skipped groups still affect Clang's per-file lexer
#endif
int after = AFTER;
const char *literal = "// retained in a string";
#define SLASH /
SLASH/ retained_tokens
