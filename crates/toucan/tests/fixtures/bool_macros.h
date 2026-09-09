enum E { ENUM_FALSE = (_Bool)0, ENUM_TRUE = (_Bool)1 };
enum { SELF = (_Bool)1 };
#define SELF SELF
#define B_FALSE ((_Bool)0)
#define B_TRUE ((_Bool)-3)
#define B_ALIAS B_TRUE
#define B_GENERIC _Generic(1, int: ((_Bool)1))
#define B_KEEP ((_Bool)1)
#define B_OVERRIDE ((_Bool)0)
#define B_GROUP_KEEP ((_Bool)0)
#define B_GROUP_OTHER ((_Bool)1)
#define ENUM_ALIAS ENUM_TRUE
#define COMPARE (1 < 2)
#define LOGICAL (((_Bool)1) && ((_Bool)1))
#define NEGATE (!((_Bool)0))
#define CONDITIONAL (1 ? ((_Bool)1) : ((_Bool)0))
#define UCHAR ((unsigned char)1)
#define true 1
#define B_QUERY __atomic_always_lock_free(4,0)
#define type ((_Bool)1)
