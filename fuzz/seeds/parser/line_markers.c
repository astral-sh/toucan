# 12 "first.h"
typedef unsigned long size;
# 3 "second.h"
int f(size n) { return n ? "é𝄞"[0] : '\x41'; }
