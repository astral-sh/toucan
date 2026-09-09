typedef Value;
static helper(a,b) int b; { return a+b; }
int entry(void) {
    int restrict=3;
    { later(1.0f); }
    return helper(restrict,later(2.0f));
}
int later(double value) { return value; }
_Static_assert((sizeof(long)==8 ? __builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long) : __builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long long)),"C90 decimal");
