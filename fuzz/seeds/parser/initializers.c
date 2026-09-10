struct S { int first; int rest[4]; };
struct S values[] = { [2] = { .rest = { [1 ... 3] = 7 }, .first = 1 } };
int f(void) { return ((struct S) { .first = 2, .rest = {3, 4} }).rest[0]; }
