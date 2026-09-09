struct Opaque;
struct Plain { int integer; double real; int values[40]; struct Opaque *opaque; void (*callback)(void); };
struct Nested { struct Plain value; };
union Choice { int integer; double real; };
struct UnionHolder { union Choice value; };
struct LargeCallback { void (*callback)(int,int,int,int,int,int,int,int,int,int,int,int,int); };
struct Atomic { _Atomic(int) value; };
struct Packed { char head; int value; } __attribute__((packed));
