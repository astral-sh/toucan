typedef int aligned_int __attribute__((aligned(16)));
typedef _Atomic(aligned_int) AtomicInt;
struct Three { char bytes[3]; };
union Value { int number; float real; };
_Atomic(struct Three) atomic_three;
_Atomic(union Value) atomic_union = (union Value){.number=2};
int atomic_operations(_Atomic(short) *p, _Atomic(float) *f) {
  int value = *p;
  *p = value; *p += 2; (*p)++; ++(*p); *f += 1.0f;
  *p; sizeof(*p); __builtin_constant_p(*p);
  return value;
}
