typedef int A __attribute__((aligned(32)));
typedef A B;
typedef int A __attribute__((aligned(64)));
typedef int Independent __attribute__((aligned(16)));
int combine(A a, B b, Independent c) {
    return __builtin_elementwise_min(a, b) + (a & c);
}
