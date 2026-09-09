__attribute__((gnu_inline)) extern __inline__ int inline_choice(int x) { return x + 1; }
int inline_choice(int x) { return x + 2; }
__inline__ int mode_dependent(int x) { return inline_choice(x); }
int inline_user(void) { extern int mode_dependent(int); return mode_dependent(3); }
extern int mode_dependent(int);
__inline__ __attribute__((weak)) int inline_optional(int x) { return x; }
__inline__ int late_weak(int x) { return x; }
int late_weak(int) __attribute__((weak));
