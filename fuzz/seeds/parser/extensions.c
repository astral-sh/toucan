typedef int vector __attribute__((vector_size(16)));
__declspec(align(16)) int __cdecl f(int value) {
    __auto_type local = ({ int x = value; x + 1; });
    __asm__ volatile ("" : "+r" (local) : : "memory");
    return __builtin_types_compatible_p(__typeof__(local), int);
}
