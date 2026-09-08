static _Complex double backing;
static int calls;
static _Complex double *address(void) { ++calls; return &backing; }
int exercise_complex_nontemporal(void) {
    _Complex double value=1.25+2.5i;
    __builtin_nontemporal_store(value,address());
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (backing != value || calls != 1) return 1;
    (void)__builtin_nontemporal_load(address());
    if (calls != 2) return 2;
    volatile _Complex double input=3.25+4.5i;
    const volatile _Complex double *qualified=&backing;
    __builtin_nontemporal_store(input,qualified);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (backing != input) return 3;
    double real;
    __builtin_nontemporal_store(value,&real);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (real != 1.25) return 4;
    return 0;
}
