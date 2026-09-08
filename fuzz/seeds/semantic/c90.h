typedef Value;
static helper(a,b) int b; { return a+b; }
int entry(void) { return later(helper(1,2)); }
int later(int value) { return value; }
_Static_assert((sizeof(long)==8 ? __builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long) : __builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long long)),"C90 decimal");
