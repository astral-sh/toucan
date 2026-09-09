struct Bytes { char v[3]; };
void atomics(volatile int *p, int *q, int **pointer, struct Bytes *a, struct Bytes *b, int order) {
    __atomic_store_n(p, 7, order);
    __atomic_fetch_add(p, 2, 5);
    __atomic_compare_exchange_n(p, q, 9, 0, 5, 2);
    __atomic_fetch_add(pointer, 1, 0);
    __atomic_load(a, b, 2);
    __atomic_exchange(a, b, b, 5);
    __atomic_compare_exchange(a, b, b, 0, 5, 2);
    __atomic_test_and_set(p, 5);
    __atomic_clear(p, 3);
    __atomic_thread_fence(5);
    __atomic_signal_fence(5);
    __atomic_always_lock_free(16, q++);
    __atomic_is_lock_free(4, q++);
}
