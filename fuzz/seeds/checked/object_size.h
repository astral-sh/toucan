unsigned long query(void *p) { return __builtin_object_size(p, 0) + __builtin_dynamic_object_size(p, 1); }
