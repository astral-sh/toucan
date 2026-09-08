#define OMITTED (7 ?: 9)
int object;
int *saved = &object ?: 0;
int calls;
int take(int x){++calls;return x;}
int choose(int x,int y){return take(x) ?: take(y);}
unsigned promote(volatile short*p,unsigned fallback){return *p ?: fallback;}
int atomic_value(_Atomic(int)*p){return *p ?: 3;}
double rounded=16777217.0f ?: 0.0;
int vla(int n,int m,int(*q)[m]){int a[2][n];return sizeof(*(a ?: q));}
