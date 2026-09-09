typedef int I __attribute__((vector_size(16)));
typedef unsigned char B __attribute__((vector_size(16)));
extern I runtime;
I selected = 1 ? (I){1, 2, 3, 4} : runtime;
I chosen = __builtin_choose_expr(0, runtime, (I){1, 2});
I generic = _Generic(1, int: +(I){1, 2, 3, 4}, default: runtime);
