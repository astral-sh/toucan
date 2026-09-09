typedef int Scalar;
struct S { struct { Scalar value; }; unsigned bits:3; };
int use(int n, int values[static n]) {
    typedef int Row[n];
    Row local;
    typeof(Row) other;
    struct S a[4] = { [2].value = 3, [0] = { .bits = 4 } };
    return sizeof local + sizeof other + values[0] + a[2].value;
}
