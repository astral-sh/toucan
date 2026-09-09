void *f(void*p,unsigned long n){p=__builtin_malloc(n);p=__builtin_calloc(n,2);p=__builtin_realloc(p,n);__builtin_free(p);return p;}
_Static_assert(!__builtin_constant_p(__builtin_calloc(1,4)),"allocation effects");
void *aliased_allocation(void) {extern void *__builtin_malloc(unsigned long) __asm__("custom_allocate");return __builtin_malloc(8);}
