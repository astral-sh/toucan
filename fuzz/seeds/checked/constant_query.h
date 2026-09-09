int helper(void);
int f(int n) { int a[2]; enum { known = __builtin_constant_p(1 + 2) }; return known + __builtin_constant_p(n++) + __builtin_constant_p(a) + __builtin_constant_p(0 && helper()); }
