int f(int x) { switch (x) { case 0: if (x) __attribute__((fallthrough)); else __attribute__((fallthrough)); case 1: return x; } return 0; }
