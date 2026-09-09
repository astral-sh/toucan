typedef int Value __attribute__((nodebug));
struct Callbacks { int (*read)(void) __attribute__((nodebug)); int ignored __attribute__((nodebug(1))); };
int read(Value value __attribute__((nodebug))) __attribute__((__nodebug__));
__attribute__((nodebug)) int read(Value value) { int local __attribute__((nodebug)) = value; return local; }
