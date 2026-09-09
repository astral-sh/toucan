int sink(int, ...);
extern inline __attribute__((gnu_inline, always_inline)) int forward(int n, ...) {
    sink(n, 1.5f, (int)__builtin_va_arg_pack());
    return __builtin_va_arg_pack_len();
}
