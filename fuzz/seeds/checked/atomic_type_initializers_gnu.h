struct Three { char values[3]; };
union Value { int number; float real; };
_Atomic(struct Three) atomic_record = {{1,2,3}};
_Atomic(union Value) atomic_union = {.number=5};
struct Host { _Atomic(struct Three) record; _Atomic(int) values[2]; } host={{{4,5,6}},{7,8}};
int load(void) {struct Three value=atomic_record;union Value other=atomic_union;return value.values[0]+other.number;}
