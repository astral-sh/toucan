typedef int A[4]; typedef int B[8]; void original(void(*callback)(A)); void original(void(*callback)(B));
__typeof__(original) copied; extern void(*pointer)(A); __typeof__(*pointer) dereferenced;
struct Holder { __typeof__(void(A)) *callback; };
