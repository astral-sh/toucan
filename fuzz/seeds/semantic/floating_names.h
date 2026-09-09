typedef int _Float128;
_Float128 outer;
int f(int _Float64){typedef char _Float128;_Float128 local=1;return local+_Float64;}
_Static_assert(sizeof(outer)==sizeof(int),"outer alias");
