void independent(int a[][*]);
void another(int a[][*]);
typedef void Shared(int a[][*]);
Shared first, second;
void arrays(int n, int condition) {
    int a[n], b[n];
    typedef int A[n];
    A x, y;
    typeof(a) reused;
    __auto_type pointer = &a;
    __auto_type composite = condition ? &a : &b;
    _Static_assert(__builtin_types_compatible_p(typeof(a), typeof(b)), "compatible VLAs");
    (void)sizeof(typeof(reused));
    (void)sizeof *pointer;
    (void)sizeof *composite;
    (void)sizeof x;
    (void)sizeof y;
}
