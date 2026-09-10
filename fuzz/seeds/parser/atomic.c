typedef int T;
_Atomic(T *) pointer;
const _Atomic T value;
int apply(int (*callback)(_Atomic(int)));
_Static_assert(sizeof(_Atomic(T)) >= sizeof(T), "atomic size");
