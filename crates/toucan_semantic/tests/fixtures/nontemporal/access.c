static int address_calls, value_calls, backing;
static int *address(void) { ++address_calls; return &backing; }
static double value(void) { ++value_calls; return 23.75; }
static int bound_calls;
static int bound(void) { ++bound_calls; return 1; }

int exercise_nontemporal(void) {
    __builtin_nontemporal_store(value(), address());
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (address_calls != 1 || value_calls != 1 || backing != 23) return 1;
    int loaded = __builtin_nontemporal_load(address());
    if (loaded != 23 || address_calls != 2) return 2;

    /* Qualifying a pointer does not make its underlying mutable object const. */
    const volatile int *qualified = &backing;
    __builtin_nontemporal_store(42, qualified);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (__builtin_nontemporal_load(qualified) != 42) return 3;
    const int immutable = 71;
    if (__builtin_nontemporal_load(&immutable) != 71) return 4;

    volatile int v = 19;
    int *volatile p = &backing;
    __builtin_nontemporal_store(v, p);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (__builtin_nontemporal_load(p) != 19) return 5;
    _Bool truth = 0;
    __builtin_nontemporal_store(9, &truth);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (__builtin_nontemporal_load(&truth) != 1) return 6;
    int *pointer = 0;
    __builtin_nontemporal_store(&backing, &pointer);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (__builtin_nontemporal_load(&pointer) != &backing) return 7;

    typedef unsigned U __attribute__((vector_size(16)));
    typedef float F __attribute__((vector_size(16)));
    U bits;
    __builtin_nontemporal_store((F){1.0f, 2.0f, 4.0f, 8.0f}, &bits);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    U read = __builtin_nontemporal_load(&bits);
    if (read[0] != 0x3f800000u || read[1] != 0x40000000u ||
        read[2] != 0x40800000u || read[3] != 0x41000000u) return 8;

    int n = 3, data[n];
    int (*array)[n] = &data;
    if (__builtin_nontemporal_load(&array) != &data) return 9;
    (void)__builtin_object_size((int (*)[bound()])__builtin_nontemporal_load(&array), 0);
    if (bound_calls != 0) return 10;
    int changes = 0;
    if (__builtin_constant_p((__builtin_nontemporal_store(changes++, (int*)0), 0))) return 11;
    if (changes != 0) return 12;
    return 0;
}
