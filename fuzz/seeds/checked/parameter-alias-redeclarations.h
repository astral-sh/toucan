typedef int A[4]; typedef int B[8]; void direct(A); void direct(B);
void nested(void(*first)(A)); void nested(void(*second)(B));
extern void(*variable)(A); extern void(*variable)(B);
