struct __attribute__((packed)) S { char x; int y; };
int aligned __attribute__((aligned(32)));
int f(int n) {
  typedef int A[n++];
  return __alignof__(aligned) + __alignof__(*((int*)(char*)&aligned))
    + __alignof__(((struct S*)0)->y) + _Alignof(_Generic(aligned, int: aligned))
    + _Alignof(int[n++]) + __alignof__(({ A a; a; }));
}
