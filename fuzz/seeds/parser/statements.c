int f(int n) {
    int value = 0;
again:
    for (int i = 0; i < n; ++i) {
        switch (i) { case 1 ... 3: value++; break; default: continue; }
    }
    do { --n; } while (n > 3);
    if (n) { n = 0; goto again; } else return value;
}
