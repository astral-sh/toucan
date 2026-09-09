typedef int A[4]; typedef int B[8]; void original(A);
void parameters(int (*original)(int), __typeof__(original) value, void(*callback)(B));
void body(void) { typedef double A[3]; void local(A); struct Local { void(*callback)(A); }; }
void after(A);
