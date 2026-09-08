typedef unsigned Count;
typedef int Aligned __attribute__((aligned(32)));
struct Payload { Count count; int value; };
enum { KNOWN = 7 };
_Noreturn void stop(void);
#define ARITHMETIC (1 + 2 * 3)
#define BOUNDARY (0xffffffffffffffffULL + 1)
#define FLOATING (0x1p-149f / 2.0f)
#define CHARACTER '['
#define CONTEXT ((Count)KNOWN + sizeof(struct Payload))
#define LOCAL_SIZE sizeof(enum Local { LOCAL_MEMBER = 1 })
#define MISSING_LOCAL sizeof(enum Local)
#define MISSING_MEMBER LOCAL_MEMBER
