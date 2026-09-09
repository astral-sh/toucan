void f(int n, int a[static restrict n]) { typedef int Row[n]; Row b; int (*p)[n] = &b; for (int v[n]; n; --n) { v[0] = sizeof *p; } goto done; done: a[0]=sizeof b; }
