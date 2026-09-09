typedef unsigned Count;
enum Flags { FIRST = 1, SECOND = 2, BOTH = FIRST | SECOND };
enum Signed { NEGATIVE = -1, SMALL = 7 };
int object;
int scoped(void) { enum { SMALL = 99 }; return SMALL; }
#define FLAG_ALIAS FIRST
#define FLAG_COMBINATION (FIRST | SECOND)
#define SIGNED_ALIAS NEGATIVE
#define WIDE_VALUE (~0ULL | SMALL)
#define FLOAT_VALUE (SMALL + 0.1f)
#define CONDITIONAL_VALUE (SMALL ? NEGATIVE : FIRST)
#define REUSED_VALUE (SMALL ?: BOTH)
#define UNKNOWN_VALUE (SMALL + missing)
#define BAD_DIVISION (SMALL / 0)
#define TYPE_ALIAS Count
#define TYPE_CAST ((Count)SMALL)
#define LOCAL_TYPE sizeof(enum QueryLocal { QUERY_MEMBER = SMALL })
#define LOCAL_MEMBER QUERY_MEMBER
