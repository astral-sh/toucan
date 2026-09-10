typedef int T;
int (*factory(int T))(int);
T after;
int f(int (*callback)(int T)) { __typeof__(T) value; return callback(value); }
int g(int T) { __typeof__(T) value = T; return value; }
