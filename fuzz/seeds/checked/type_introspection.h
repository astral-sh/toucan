struct Item { int bits:3; int value; };
int helper(int);
int inspect(int n, int m) {
    typedef int Row[n++];
    int values[__builtin_types_compatible_p(Row, int[])];
    __builtin_choose_expr(1, values[0], (void)0) = 7;
    return __builtin_choose_expr(
        __builtin_types_compatible_p(int[n++], typeof(*(int (*)[m++])0)),
        __builtin_constant_p(__builtin_choose_expr(1, sizeof(int[n++]), n++)),
        helper(__builtin_choose_expr(0, sizeof(int[m++]), values[0])));
}
