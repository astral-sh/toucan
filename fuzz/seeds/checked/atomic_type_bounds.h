void atomic_bounds(int n, int values[_Atomic n]) {
  typedef int (*P)[n++];
  _Atomic(P) pointer;
  P loaded = pointer;
  int (* _Atomic other)[n++];
  (void)(_Atomic(int(*)[n++]))0;
  int storage[2][n]; _Atomic(int(*)[n]) initialized=storage;
  pointer=loaded;
}
