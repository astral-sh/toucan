void prototype(int a[*][*]);
void definition(int n, int a[n][n]) {
    typedef int Row[n];
    Row local;
    int (*pointer)[n] = a;
    unsigned long size = sizeof(local);
    enum { ALIGN = _Alignof(int[n]) };
    local[0] = pointer[0][0];
    goto done;
    done:;
}
