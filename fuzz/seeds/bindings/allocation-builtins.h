void *f(void*p,unsigned long n){p=__builtin_malloc(n);p=__builtin_calloc(n,2);p=__builtin_realloc(p,n);__builtin_free(p);return p;}
