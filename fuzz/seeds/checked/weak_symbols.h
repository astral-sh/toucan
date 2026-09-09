extern int optional;
void declare(void) { extern int optional __attribute__((weak)); if (&optional) optional = 3; }
extern int optional;
int __attribute__((weak)) replaceable(void) { return 7; }
int inspect(void) { return replaceable ? replaceable() : 0; }
