/* Clang promises are C-compatible and intersect on visible redeclaration. */
typedef void A(int *__attribute__((noescape)),int *__attribute__((noescape)),int*);
typedef void B(int*,int *__attribute__((noescape)),int *__attribute__((noescape)));
void f(int c,A*a,B*b,int*p) { __typeof__(c?a:b) v=c?a:b; v(p,p,p); }
void callee(int *p __attribute__((noescape)));
void early(int *p) { callee(p); }
void callee(int *p);
void late(int *p) { callee(p); }
void old(p) int *p __attribute__((noescape)); { *p=1; }
typedef void Stop(int*) __attribute__((noreturn));
typedef void Plain(int*);
void crossing(Stop*s, A*a) { (void)s; (void)a; }
void merged(int*) __attribute__((noreturn));
void merged(int*);
