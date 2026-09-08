int fixed_destination(int n, int (*p)[n]) {
    int (*q)[3] = p;
    return sizeof *q;
}
int typeof_pointer_update(int n) {
    int a[n];
    int (*p)[n] = &a;
    typeof(p++) q;
    return sizeof *q;
}
int composite_extent(int n, int m, int (*p)[n], int (*q)[m]) {
    return sizeof *(n ? p : q);
}
int callback_contract(int n, int a[static 3], int (*callback)(int b[static n])) {
    return callback(a);
}
