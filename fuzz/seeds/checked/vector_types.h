typedef int V __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
struct S { char x; V v; };
V f(V a, V b, F f, long shift) {
 V v = {1, 2}; v += a; v = a < b; v = v << shift;
 v = (V)(f + 1.0); v[0] = ((V){1,2,3,4})[1];
 return shift ? v : -b;
}
_Static_assert(sizeof(struct S)==32, "layout");
