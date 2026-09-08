struct Pair { int x, y; }; struct Pair large[1000000] = { [999999].y = 3 };
int f(void) { struct Pair p = {.y=2, .x=1}; return (struct Pair){.x=p.y}.x; }
