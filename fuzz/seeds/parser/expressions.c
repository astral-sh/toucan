struct S { int value; int array[4]; };
int call(int, int);
int f(int a, int b, int c, struct S *s) {
    a += b * c << 2;
    return _Generic(a, int: call(a, b), default: s->array[a])
        ? __builtin_choose_expr(1, a ?: b, c)
        : __builtin_offsetof(struct S, array[1]);
}
