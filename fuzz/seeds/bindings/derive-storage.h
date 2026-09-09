typedef enum { FORM_COMPACT=2, FORM_FULL=4, FORM_HYBRID=6 } Form;
enum Zero { ZERO=0, ONE=1 };
struct NonZero { Form value; };
struct ZeroDefault { enum Zero value; void *pointer; int (*callback)(void); int bytes[33]; };
union UnselectedMember { Form form; struct ZeroDefault value; };
struct Aggregate { union UnselectedMember value; struct NonZero *optional; };
struct Packed { int field; } __attribute__((packed));
