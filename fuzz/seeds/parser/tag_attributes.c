struct __attribute__((packed, aligned(2))) S { char byte; int value; };
union __attribute__((aligned(8))) U { long value; char bytes[8]; };
enum __attribute__((packed)) E { E0, E1 = 3 };
typedef struct S __attribute__((aligned(16))) S;
struct S make(void) { return (struct S) { .byte = 1, .value = 2 }; }
